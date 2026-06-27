package com.dbx.agent.oracle;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.ConnectParams;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

import java.sql.SQLException;
import java.util.List;
import java.util.Locale;

class OracleAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new OracleAgent();
    }

    @Override
    protected String resultSetSql() {
        return "CALL DBMS_XPLAN.DISPLAY_CURSOR()";
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

        Assertions.assertEquals("jdbc:oracle:thin:@oracle.example.com:1521:ORCL", OracleAgent.buildUrl(params));
    }

    @Test
    void prepareExecutableSqlKeepsPlsqlObjectTerminator() {
        String sql = "CREATE OR REPLACE PROCEDURE APP_PROC AS BEGIN NULL; END;";

        Assertions.assertEquals(sql, OracleAgent.prepareExecutableSql(sql));
    }

    @Test
    void prepareExecutableSqlTrimsPlainStatementTerminator() {
        Assertions.assertEquals("SELECT 1 FROM DUAL", OracleAgent.prepareExecutableSql("SELECT 1 FROM DUAL;"));
    }

    @Test
    void prepareExecutableSqlStillRewritesFetchFirst() {
        Assertions.assertEquals(
            "SELECT * FROM (SELECT * FROM EMP) WHERE ROWNUM <= 10",
            OracleAgent.prepareExecutableSql("SELECT * FROM EMP FETCH FIRST 10 ROWS ONLY;")
        );
    }

    @Test
    void listDatabasesSqlUsesUserDictionaryInsteadOfObjectDictionary() {
        String sql = OracleAgent.listDatabasesSql().toUpperCase(Locale.ROOT);

        Assertions.assertTrue(sql.contains("ALL_USERS"));
        Assertions.assertFalse(sql.contains("ALL_TABLES"));
        Assertions.assertFalse(sql.contains("ALL_VIEWS"));
    }

    @Test
    void listDatabasesSqlCanApplyVisibleSchemaFilter() {
        String sql = OracleAgent.listDatabasesSql(2).toUpperCase(Locale.ROOT);

        Assertions.assertTrue(sql.contains("ALL_USERS"), sql);
        Assertions.assertTrue(sql.contains("USERNAME IN (?,?)"), sql);
        Assertions.assertFalse(sql.contains("ALL_TABLES"), sql);
    }

    @Test
    void listSchemasWithEmptyVisibleFilterSkipsMetadataQuery() {
        OracleAgent agent = new OracleAgent();

        Assertions.assertEquals(List.of(), agent.listSchemas(List.of()));
    }

    @Test
    void listTablesSqlUsesSplitDictionaryQuery() {
        String sql = OracleAgent.listTablesSql().toUpperCase(Locale.ROOT);

        Assertions.assertTrue(sql.contains("ALL_TABLES"), sql);
        Assertions.assertTrue(sql.contains("ALL_OBJECTS"), sql);
        Assertions.assertTrue(sql.contains("UNION ALL"), sql);
        Assertions.assertFalse(sql.contains("ALL_TAB_COMMENTS"), sql);
    }

    @Test
    void listObjectsSqlUsesSplitDictionaryQuery() {
        String sql = OracleAgent.listObjectsSql().toUpperCase(Locale.ROOT);

        Assertions.assertTrue(sql.contains("ALL_TABLES"), sql);
        Assertions.assertTrue(sql.contains("ALL_OBJECTS"), sql);
        Assertions.assertTrue(sql.contains("UNION ALL"), sql);
        Assertions.assertFalse(sql.contains("ALL_TAB_COMMENTS"), sql);
    }

    @Test
    void detectsPgaLimitError() {
        Assertions.assertTrue(OracleAgent.isPgaLimitError(new SQLException(
            "ORA-04036: PGA memory used by the instance exceeds PGA_AGGREGATE_LIMIT",
            "72000",
            4036
        )));
    }
}
