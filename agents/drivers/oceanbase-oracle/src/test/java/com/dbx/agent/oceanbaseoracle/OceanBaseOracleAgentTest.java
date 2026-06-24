package com.dbx.agent.oceanbaseoracle;

import com.dbx.agent.ConnectParams;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class OceanBaseOracleAgentTest {
    @Test
    void buildsOceanBaseJdbcUrl() {
        ConnectParams params = new ConnectParams();
        params.setHost("oceanbase.example.com");
        params.setPort(0);
        params.setDatabase("sys");

        Assertions.assertEquals(
            "jdbc:oceanbase://oceanbase.example.com:2881/sys?compatibleOjdbcVersion=8",
            OceanBaseOracleAgent.buildUrl(params)
        );
    }

    @Test
    void appendsQueryParametersToJdbcUrl() {
        ConnectParams params = new ConnectParams();
        params.setHost("oceanbase.example.com");
        params.setPort(2881);
        params.setDatabase("sys");
        params.setUrl_params("useSSL=false");

        Assertions.assertEquals(
            "jdbc:oceanbase://oceanbase.example.com:2881/sys?useSSL=false&compatibleOjdbcVersion=8",
            OceanBaseOracleAgent.buildUrl(params)
        );
    }

    @Test
    void keepsExplicitCompatibleOjdbcVersion() {
        ConnectParams params = new ConnectParams();
        params.setHost("oceanbase.example.com");
        params.setPort(2881);
        params.setDatabase("sys");
        params.setUrl_params("compatibleOjdbcVersion=6&useSSL=false");

        Assertions.assertEquals(
            "jdbc:oceanbase://oceanbase.example.com:2881/sys?compatibleOjdbcVersion=6&useSSL=false",
            OceanBaseOracleAgent.buildUrl(params)
        );
    }

    @Test
    void appendsCompatibleOjdbcVersionToCustomJdbcUrl() {
        ConnectParams params = new ConnectParams();
        params.setConnection_string("jdbc:oceanbase://custom-host:2881/sys?useSSL=false");

        Assertions.assertEquals(
            "jdbc:oceanbase://custom-host:2881/sys?useSSL=false&compatibleOjdbcVersion=8",
            OceanBaseOracleAgent.buildUrl(params)
        );
    }
}
