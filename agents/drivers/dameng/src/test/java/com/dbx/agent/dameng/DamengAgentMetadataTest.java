package com.dbx.agent.dameng;

import com.dbx.agent.ColumnInfo;
import com.dbx.agent.ObjectInfo;
import com.dbx.agent.TableInfo;
import com.dbx.agent.test.JdbcMetadataSqlFake;
import com.dbx.agent.test.TestSupport;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.util.Arrays;
import java.util.List;

class DamengAgentMetadataTest {
    @Test
    void usesColumnCommentsMetadataQuery() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.getColumns("APP", "USERS");

        String columnsSql = JdbcMetadataSqlFake.statements.stream()
            .filter(sql -> sql.contains("ALL_TAB_COLUMNS"))
            .findFirst()
            .orElseThrow();
        Assertions.assertTrue(columnsSql.contains("LEFT JOIN ALL_COL_COMMENTS"), columnsSql);
    }

    @Test
    void usesTableCommentsMetadataQuery() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.listTables("APP");

        String tablesSql = JdbcMetadataSqlFake.statements.stream()
            .filter(sql -> sql.contains("ALL_TAB_COMMENTS"))
            .findFirst()
            .orElseThrow();
        Assertions.assertTrue(tablesSql.contains("COMMENTS"), tablesSql);
        Assertions.assertFalse(tablesSql.contains("ALL_TABLES"), tablesSql);
        Assertions.assertFalse(tablesSql.contains("ALL_VIEWS"), tablesSql);
    }

    @Test
    void mapsTableCommentFromMetadata() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, metadataConnection());

        List<TableInfo> tables = agent.listTables("APP");

        Assertions.assertEquals(1, tables.size());
        Assertions.assertEquals("USERS", tables.get(0).getName());
        Assertions.assertEquals("用户示例表", tables.get(0).getComment());
    }

    @Test
    void mapsTableCommentToObjectMetadata() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, metadataConnection());

        List<ObjectInfo> objects = agent.listObjects("APP");

        Assertions.assertEquals(1, objects.size());
        Assertions.assertEquals("USERS", objects.get(0).getName());
        Assertions.assertEquals("用户示例表", objects.get(0).getComment());
    }

    @Test
    void triggerMetadataDoesNotRequireOracleTriggerTypeColumn() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.listTriggers("APP", "USERS");

        String triggersSql = JdbcMetadataSqlFake.statements.stream()
            .filter(sql -> sql.contains("ALL_TRIGGERS"))
            .findFirst()
            .orElseThrow();
        Assertions.assertFalse(triggersSql.contains("TRIGGERING_EVENT, TRIGGER_TYPE"), triggersSql);
        Assertions.assertTrue(triggersSql.contains("'' AS TRIGGER_TYPE"), triggersSql);
    }

    @Test
    void mapsColumnCommentFromMetadata() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, metadataConnection());

        List<ColumnInfo> columns = agent.getColumns("APP", "USERS");

        Assertions.assertEquals(1, columns.size());
        Assertions.assertEquals("id comment", columns.get(0).getComment());
    }

    @Test
    void mapsColumnCommentFromFallbackMetadataViews() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, metadataConnection(null, "fallback id comment"));

        List<ColumnInfo> columns = agent.getColumns("APP", "USERS");

        Assertions.assertEquals(1, columns.size());
        Assertions.assertEquals("fallback id comment", columns.get(0).getComment());
    }

    @Test
    void appendsTableAndColumnCommentsToTableDdl() {
        DamengAgent agent = new DamengAgent();
        TestSupport.setPrivateConnection(agent, metadataConnection());

        String ddl = agent.getTableDdl("APP", "USERS");

        Assertions.assertTrue(ddl.contains("CREATE TABLE \"APP\".\"USERS\""), ddl);
        Assertions.assertTrue(ddl.contains("COMMENT ON TABLE \"APP\".\"USERS\" IS '用户示例表';"), ddl);
        Assertions.assertTrue(ddl.contains("COMMENT ON COLUMN \"APP\".\"USERS\".\"ID\" IS 'id comment';"), ddl);
    }

    private static Connection metadataConnection() {
        return metadataConnection("id comment", null);
    }

    private static Connection metadataConnection(String allColumnComment, String fallbackColumnComment) {
        return proxy(Connection.class, (method, args) -> {
            String name = method.getName();
            if ("prepareStatement".equals(name)) {
                String sql = (String) args[0];
                if (sql.contains("DBMS_METADATA.GET_DDL")) {
                    return metadataStatement(List.of(List.of("CREATE TABLE \"APP\".\"USERS\" (\n  \"ID\" NUMBER\n);")));
                }
                if (sql.contains("ALL_CONS_COLUMNS")) {
                    return metadataStatement(List.of(List.of("ID")));
                }
                if (sql.startsWith("SELECT COMMENTS")) {
                    return metadataStatement(List.of(List.of("用户示例表")));
                }
                if (sql.contains("USER_COL_COMMENTS")) {
                    return metadataStatement(fallbackColumnComment == null ? List.of() : List.of(List.of("ID", fallbackColumnComment)));
                }
                if (sql.contains("SYSCOLUMNCOMMENTS")) {
                    return metadataStatement(List.of());
                }
                if (sql.contains("ALL_TAB_COMMENTS")) {
                    return metadataStatement(List.of(List.of("USERS", "TABLE", "用户示例表")));
                }
                if (sql.contains("ALL_COL_COMMENTS") && !sql.contains("ALL_TAB_COLUMNS")) {
                    return metadataStatement(List.of());
                }
                if (sql.contains("ALL_OBJECTS")) {
                    return metadataStatement(List.of());
                }
                if (sql.contains("ALL_TAB_COLUMNS")) {
                    return metadataStatement(List.of(Arrays.asList(
                        "ID",
                        "NUMBER",
                        "N",
                        Integer.valueOf(10),
                        Integer.valueOf(0),
                        Integer.valueOf(22),
                        Integer.valueOf(10),
                        allColumnComment
                    )));
                }
            }
            if ("close".equals(name)) {
                return null;
            }
            if ("isClosed".equals(name)) {
                return false;
            }
            return defaultValue(method.getReturnType());
        });
    }

    private static PreparedStatement metadataStatement(List<List<Object>> rows) {
        return proxy(PreparedStatement.class, (method, args) -> {
            String name = method.getName();
            if ("executeQuery".equals(name)) {
                return metadataResultSet(rows);
            }
            if ("setString".equals(name) || "close".equals(name)) {
                return null;
            }
            return defaultValue(method.getReturnType());
        });
    }

    private static ResultSet metadataResultSet(List<List<Object>> rows) {
        int[] index = {-1};
        return proxy(ResultSet.class, (method, args) -> {
            String name = method.getName();
            if ("next".equals(name)) {
                index[0] += 1;
                return index[0] < rows.size();
            }
            if ("getString".equals(name)) {
                if (args[0] instanceof Integer columnIndex) {
                    Object value = rows.get(index[0]).get(columnIndex - 1);
                    return value == null ? null : value.toString();
                }
                return switch (((String) args[0]).toUpperCase()) {
                    case "COLUMN_NAME" -> string(rows, index[0], 0);
                    case "DATA_TYPE" -> string(rows, index[0], 1);
                    case "NULLABLE" -> string(rows, index[0], 2);
                    case "DATA_DEFAULT" -> null;
                    case "COMMENTS", "COMMENT$" -> string(rows, index[0], rows.get(index[0]).size() - 1);
                    case "COLNAME" -> string(rows, index[0], 0);
                    default -> null;
                };
            }
            if ("getObject".equals(name)) {
                return switch (((String) args[0]).toUpperCase()) {
                    case "DATA_PRECISION" -> rows.get(index[0]).get(3);
                    case "DATA_SCALE" -> rows.get(index[0]).get(4);
                    case "DATA_LENGTH" -> rows.get(index[0]).get(5);
                    case "CHAR_LENGTH" -> rows.get(index[0]).get(6);
                    default -> null;
                };
            }
            if ("close".equals(name)) {
                return null;
            }
            return defaultValue(method.getReturnType());
        });
    }

    private static String string(List<List<Object>> rows, int rowIndex, int columnIndex) {
        Object value = rows.get(rowIndex).get(columnIndex);
        return value == null ? null : value.toString();
    }

    @SuppressWarnings("unchecked")
    private static <T> T proxy(Class<T> type, MethodHandler handler) {
        InvocationHandler invocationHandler = new InvocationHandler() {
            @Override
            public Object invoke(Object proxy, Method method, Object[] args) {
                return handler.handle(method, args);
            }
        };
        return (T) Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, invocationHandler);
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

    private interface MethodHandler {
        Object handle(Method method, Object[] args);
    }
}
