package com.dbx.agent.oracle10g;

import com.dbx.agent.test.TestSupport;
import com.dbx.agent.BaseDatabaseAgent;
import com.dbx.agent.ConnectParams;
import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.ObjectInfo;
import com.dbx.agent.ObjectSource;
import com.dbx.agent.test.JdbcAgentFake;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Proxy;
import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.util.Arrays;
import java.util.List;
import java.util.function.Function;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class Oracle10gAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new Oracle10gAgent();
    }

    @Override
    protected String resultSetSql() {
        return "CALL DBMS_XPLAN.DISPLAY_CURSOR()";
    }

    @Test
    void agentInheritsBaseDatabaseAgent() {
        Assertions.assertTrue(createAgent() instanceof BaseDatabaseAgent);
    }

    @Test
    void buildUrlUsesExplicitConnectionString() {
        ConnectParams params = new ConnectParams(
            "oracle.example.com",
            1521,
            "ORCL",
            "scott",
            "tiger",
            "",
            "jdbc:oracle:thin:@oracle.example.com:1521:ORCL",
            false
        );

        Assertions.assertEquals("jdbc:oracle:thin:@oracle.example.com:1521:ORCL", Oracle10gAgent.buildUrl(params));
    }

    @Test
    void prepareExecutableSqlKeepsPlsqlObjectTerminator() {
        String sql = "CREATE OR REPLACE PROCEDURE APP_PROC AS BEGIN NULL; END;";

        Assertions.assertEquals(sql, Oracle10gAgent.prepareExecutableSql(sql));
    }

    @Test
    void prepareExecutableSqlTrimsPlainStatementTerminator() {
        Assertions.assertEquals("SELECT 1 FROM DUAL", Oracle10gAgent.prepareExecutableSql("SELECT 1 FROM DUAL;"));
    }

    @Test
    void prepareExecutableSqlStillRewritesFetchFirst() {
        Assertions.assertEquals(
            "SELECT * FROM (SELECT * FROM EMP) WHERE ROWNUM <= 10",
            Oracle10gAgent.prepareExecutableSql("SELECT * FROM EMP FETCH FIRST 10 ROWS ONLY;")
        );
    }

    @Test
    void listsTablesViewsProceduresAndFunctions() {
        Oracle10gAgent agent = new Oracle10gAgent();
        TestSupport.setPrivateConnection(agent, objectListConnection());

        List<ObjectInfo> objects = agent.listObjects("APP");

        Assertions.assertEquals(
            Arrays.asList("TABLE", "VIEW", "PROCEDURE", "FUNCTION"),
            objects.stream().map(ObjectInfo::getObject_type).collect(Collectors.toList())
        );
        Assertions.assertEquals(
            Arrays.asList("APP_TABLE", "APP_VIEW", "APP_PROC", "APP_FUNC"),
            objects.stream().map(ObjectInfo::getName).collect(Collectors.toList())
        );
    }

    @Test
    void loadsRoutineSourceFromDbmsMetadata() {
        Oracle10gAgent agent = new Oracle10gAgent();
        TestSupport.setPrivateConnection(
            agent,
            metadataConnection(sql -> Arrays.asList(
                Arrays.asList("CREATE OR REPLACE PROCEDURE APP_PROC AS BEGIN NULL; END;")
            ))
        );

        ObjectSource source = agent.getObjectSource("APP", "APP_PROC", "PROCEDURE");

        Assertions.assertEquals("APP_PROC", source.getName());
        Assertions.assertEquals("PROCEDURE", source.getObject_type());
        Assertions.assertEquals("APP", source.getSchema());
        Assertions.assertEquals("CREATE OR REPLACE PROCEDURE APP_PROC AS BEGIN NULL; END;", source.getSource());
    }

    private static Connection objectListConnection() {
        return metadataConnection(sql -> {
            if (sql.contains("'PROCEDURE'")) {
                return Arrays.asList(
                    Arrays.asList("APP_TABLE", "TABLE"),
                    Arrays.asList("APP_VIEW", "VIEW"),
                    Arrays.asList("APP_PROC", "PROCEDURE"),
                    Arrays.asList("APP_FUNC", "FUNCTION")
                );
            }
            return Arrays.asList(
                Arrays.asList("APP_TABLE", "TABLE", ""),
                Arrays.asList("APP_VIEW", "VIEW", "")
            );
        });
    }

    private static Connection metadataConnection(Function<String, List<List<String>>> rowsForSql) {
        return proxy(Connection.class, (unused, method, args) -> {
            switch (method.getName()) {
                case "prepareStatement":
                    return metadataStatement(rowsForSql.apply((String) args[0]));
                case "close":
                    return null;
                case "isClosed":
                    return false;
                default:
                    return defaultValue(method.getReturnType());
            }
        });
    }

    private static PreparedStatement metadataStatement(List<List<String>> rows) {
        return proxy(PreparedStatement.class, (unused, method, args) -> {
            switch (method.getName()) {
                case "executeQuery":
                    return metadataResultSet(rows);
                case "setString":
                case "close":
                    return null;
                default:
                    return defaultValue(method.getReturnType());
            }
        });
    }

    private static ResultSet metadataResultSet(List<List<String>> rows) {
        int[] index = {-1};
        return proxy(ResultSet.class, (unused, method, args) -> {
            switch (method.getName()) {
                case "next":
                    index[0] += 1;
                    return index[0] < rows.size();
                case "getString":
                    return rows.get(index[0]).get(((Integer) args[0]) - 1);
                case "close":
                    return null;
                default:
                    return defaultValue(method.getReturnType());
            }
        });
    }

    @SuppressWarnings("unchecked")
    private static <T> T proxy(Class<T> type, InvocationHandler handler) {
        return (T) Proxy.newProxyInstance(type.getClassLoader(), new Class<?>[]{type}, handler);
    }

    private static Object defaultValue(Class<?> type) {
        if (type == Boolean.TYPE) {
            return false;
        }
        if (type == Byte.TYPE) {
            return (byte) 0;
        }
        if (type == Short.TYPE) {
            return (short) 0;
        }
        if (type == Integer.TYPE) {
            return 0;
        }
        if (type == Long.TYPE) {
            return 0L;
        }
        if (type == Float.TYPE) {
            return 0F;
        }
        if (type == Double.TYPE) {
            return 0.0D;
        }
        if (type == Character.TYPE) {
            return '\0';
        }
        return null;
    }
}
