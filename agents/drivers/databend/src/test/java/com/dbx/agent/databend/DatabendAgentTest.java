package com.dbx.agent.databend;

import com.dbx.agent.ConnectParams;
import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class DatabendAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new DatabendAgent();
    }

    @Override
    protected String resultSetSql() {
        return "SELECT 1";
    }

    @Test
    void buildsDefaultJdbcUrl() {
        ConnectParams params = new ConnectParams();
        params.setHost("db.example.com");
        params.setPort(0);
        params.setDatabase("default");

        Assertions.assertEquals(
            "jdbc:databend://db.example.com:8000/default",
            DatabendAgent.DATABEND_PROFILE.buildUrl(params)
        );
    }

    @Test
    void appendsQueryParametersToJdbcUrl() {
        ConnectParams params = new ConnectParams();
        params.setHost("db.example.com");
        params.setPort(20080);
        params.setDatabase("default");
        params.setUrl_params("sslmode=disable");

        Assertions.assertEquals(
            "jdbc:databend://db.example.com:20080/default?sslmode=disable",
            DatabendAgent.DATABEND_PROFILE.buildUrl(params)
        );
    }

    @Test
    void usesDoubleQuotedDatabaseAsExecutionContext() {
        Assertions.assertEquals(
            "USE \"analytics\"\"prod\"",
            DatabendAgent.DATABEND_PROFILE.schemaSwitchSql("analytics\"prod")
        );
    }
}
