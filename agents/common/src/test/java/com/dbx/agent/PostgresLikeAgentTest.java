package com.dbx.agent;

import org.junit.jupiter.api.Test;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.Types;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class PostgresLikeAgentTest {
    @Test
    void metadataQueriesUsePgCatalogInsteadOfInformationSchema() {
        TestPostgresLikeAgent agent = new TestPostgresLikeAgent();
        agent.connect(new ConnectParams());

        agent.listSchemas();
        agent.listTables("app");
        agent.listObjects("app");
        agent.getColumns("app", "orders");
        agent.listForeignKeys("app", "orders");
        agent.listTriggers("app", "orders");

        String sql = String.join("\n", MetadataSqlFake.statements);

        assertFalse(sql.contains("FROM information_schema"), sql);
        assertFalse(sql.contains("JOIN information_schema"), sql);
        assertTrue(sql.contains("pg_catalog.pg_namespace"), sql);
        assertTrue(sql.contains("pg_catalog.pg_class"), sql);
        assertTrue(sql.contains("pg_catalog.pg_proc"), sql);
        assertTrue(sql.contains("pg_catalog.pg_attribute"), sql);
        assertTrue(sql.contains("pg_catalog.pg_constraint"), sql);
        assertTrue(sql.contains("pg_catalog.pg_trigger"), sql);
        assertFalse(sql.contains(" AS key "), sql);
        assertFalse(sql.contains(" key."), sql);
    }

    @Test
    void postgisGeometryTypeNameDetection() {
        assertTrue(PostgresLikeAgent.isPostgisGeometryTypeName("geometry"));
        assertTrue(PostgresLikeAgent.isPostgisGeometryTypeName("GEOMETRY"));
        assertTrue(PostgresLikeAgent.isPostgisGeometryTypeName(" Geography "));
        assertTrue(PostgresLikeAgent.isPostgisGeometryTypeName("public.geometry"));
        assertTrue(PostgresLikeAgent.isPostgisGeometryTypeName("geometry(Point,4326)"));
        assertFalse(PostgresLikeAgent.isPostgisGeometryTypeName(""));
        assertFalse(PostgresLikeAgent.isPostgisGeometryTypeName(null));
        assertFalse(PostgresLikeAgent.isPostgisGeometryTypeName("text"));
        assertFalse(PostgresLikeAgent.isPostgisGeometryTypeName("vector"));
    }

    @Test
    void executeQueryDecodesGeometryColumnsToWktAndReportsColumnTypes() {
        TestPostgresLikeAgent agent = new TestPostgresLikeAgent();
        agent.connect(new ConnectParams());

        QueryResult result = JdbcExecutor.INSTANCE.readResultSet(
            GeometryResultSet.create(),
            5L,
            JdbcExecutor.DEFAULT_MAX_ROWS,
            agent.geometryAwareResolverForTest()
        );

        assertEquals(2, result.getColumns().size());
        assertEquals("id", result.getColumns().get(0));
        assertEquals("geom", result.getColumns().get(1));

        // column_types is reported via JDBC getColumnTypeName
        assertEquals(2, result.getColumn_types().size());
        assertEquals("int4", result.getColumn_types().get(0));
        assertEquals("geometry", result.getColumn_types().get(1));

        // The geometry cell is decoded to WKT (matches Rust ewkb_to_wkt fixture).
        assertEquals(1, result.getRows().size());
        assertEquals(1, result.getRows().get(0).get(0));
        assertEquals("POINT(116.397 39.908)", result.getRows().get(0).get(1));
    }

    private static final class TestPostgresLikeAgent extends PostgresLikeAgent {
        private TestPostgresLikeAgent() {
            super(new PostgresLikeAgentProfile(
                PostgresLikeAgentTest.class.getName(),
                "jdbc:test://{host}:{port}/{database}"
            ));
        }

        @Override
        protected Connection openConnection(ConnectParams params) {
            return MetadataSqlFake.connection();
        }

        JdbcExecutor.ColumnAwareResultValueReader geometryAwareResolverForTest() {
            return (rs, index, sqlType, columnTypeName) -> {
                if (PostgresLikeAgent.isPostgisGeometryTypeName(columnTypeName)) {
                    Object raw = rs.getObject(index);
                    if (rs.wasNull() || raw == null) {
                        return null;
                    }
                    return EwkbWktDecoder.decode(raw);
                }
                if (sqlType == Types.INTEGER) {
                    return rs.getInt(index);
                }
                return rs.getObject(index);
            };
        }
    }

    private static final class GeometryResultSet {
        static ResultSet create() {
            ResultSetMetaData meta = (ResultSetMetaData) Proxy.newProxyInstance(
                GeometryResultSet.class.getClassLoader(),
                new Class<?>[]{ResultSetMetaData.class},
                (proxy, method, args) -> {
                    switch (method.getName()) {
                        case "getColumnCount":
                            return 2;
                        case "getColumnLabel":
                        case "getColumnName":
                            return ((Integer) args[0]) == 1 ? "id" : "geom";
                        case "getColumnType":
                            return ((Integer) args[0]) == 1 ? Types.INTEGER : Types.OTHER;
                        case "getColumnTypeName":
                            return ((Integer) args[0]) == 1 ? "int4" : "geometry";
                        default:
                            return null;
                    }
                }
            );

            // POINT(116.397 39.908) with SRID=4326, little-endian.
            String hex = "0101000020E6100000C520B07268195D404E62105839F44340";
            byte[] geomBytes = parseHex(hex);

            int[] cursor = new int[]{0}; // 0 = before first row
            return (ResultSet) Proxy.newProxyInstance(
                GeometryResultSet.class.getClassLoader(),
                new Class<?>[]{ResultSet.class},
                (proxy, method, args) -> {
                    String name = method.getName();
                    switch (name) {
                        case "getMetaData":
                            return meta;
                        case "next":
                            cursor[0] += 1;
                            return cursor[0] == 1;
                        case "getInt":
                            return cursor[0] == 1 ? 1 : 0;
                        case "getObject":
                            return ((Integer) args[0]) == 2 ? geomBytes : 1;
                        case "wasNull":
                            return false;
                        case "close":
                            return null;
                        default:
                            return defaultPrimitive(method.getReturnType());
                    }
                }
            );
        }

        private static byte[] parseHex(String s) {
            byte[] out = new byte[s.length() / 2];
            for (int i = 0; i < out.length; i++) {
                int hi = Character.digit(s.charAt(i * 2), 16);
                int lo = Character.digit(s.charAt(i * 2 + 1), 16);
                out[i] = (byte) ((hi << 4) | lo);
            }
            return out;
        }

        private static Object defaultPrimitive(Class<?> t) {
            if (Boolean.TYPE.equals(t)) return false;
            if (Integer.TYPE.equals(t)) return 0;
            if (Long.TYPE.equals(t)) return 0L;
            if (Double.TYPE.equals(t)) return 0.0;
            return null;
        }
    }

    private static final class MetadataSqlFake {
        private static final List<String> statements = new ArrayList<String>();

        private static Connection connection() {
            statements.clear();
            return proxy(Connection.class, new MethodHandler() {
                @Override
                public Object handle(Method method, Object[] args) {
                    String name = method.getName();
                    if ("prepareStatement".equals(name)) {
                        statements.add((String) args[0]);
                        return preparedStatement();
                    }
                    if ("isClosed".equals(name)) {
                        return false;
                    }
                    if ("close".equals(name)) {
                        return null;
                    }
                    return defaultValue(method.getReturnType());
                }
            });
        }

        private static PreparedStatement preparedStatement() {
            final ResultSet resultSet = emptyResultSet();
            return proxy(PreparedStatement.class, new MethodHandler() {
                @Override
                public Object handle(Method method, Object[] args) {
                    String name = method.getName();
                    if ("executeQuery".equals(name)) {
                        return resultSet;
                    }
                    if ("setString".equals(name)) {
                        statements.add("param:" + args[0] + "=" + args[1]);
                        return null;
                    }
                    if ("close".equals(name)) {
                        return null;
                    }
                    return defaultValue(method.getReturnType());
                }
            });
        }

        private static ResultSet emptyResultSet() {
            final ResultSetMetaData metadata = proxy(ResultSetMetaData.class, new MethodHandler() {
                @Override
                public Object handle(Method method, Object[] args) {
                    if ("getColumnCount".equals(method.getName())) {
                        return 0;
                    }
                    return defaultValue(method.getReturnType());
                }
            });
            return proxy(ResultSet.class, new MethodHandler() {
                @Override
                public Object handle(Method method, Object[] args) {
                    String name = method.getName();
                    if ("next".equals(name)) {
                        return false;
                    }
                    if ("getMetaData".equals(name)) {
                        return metadata;
                    }
                    if ("close".equals(name)) {
                        return null;
                    }
                    return defaultValue(method.getReturnType());
                }
            });
        }

        private static <T> T proxy(Class<T> type, final MethodHandler handler) {
            InvocationHandler invocationHandler = new InvocationHandler() {
                @Override
                public Object invoke(Object proxy, Method method, Object[] args) {
                    return handler.handle(method, args);
                }
            };
            return type.cast(Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, invocationHandler));
        }

        private static Object defaultValue(Class<?> type) {
            if (Boolean.TYPE.equals(type)) {
                return false;
            }
            if (Byte.TYPE.equals(type)) {
                return (byte) 0;
            }
            if (Short.TYPE.equals(type)) {
                return (short) 0;
            }
            if (Integer.TYPE.equals(type)) {
                return 0;
            }
            if (Long.TYPE.equals(type)) {
                return 0L;
            }
            if (Float.TYPE.equals(type)) {
                return 0f;
            }
            if (Double.TYPE.equals(type)) {
                return 0.0d;
            }
            if (Character.TYPE.equals(type)) {
                return '\0';
            }
            return null;
        }
    }

    private interface MethodHandler {
        Object handle(Method method, Object[] args);
    }
}
