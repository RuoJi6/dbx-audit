package com.dbx.agent.gbase8s;

import com.dbx.agent.ConnectParams;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class Gbase8sAgentTest {
    @Test
    void declaresGbase8sProfile() {
        Gbase8sAgent agent = new Gbase8sAgent();

        Assertions.assertEquals("com.gbasedbt.jdbc.Driver", agent.getProfile().getDriverClass());
        Assertions.assertEquals("jdbc:gbasedbt-sqli://{host}:{port}/{database}:GBASEDBTSERVER=gbase8s", agent.getProfile().getUrlTemplate());
        Assertions.assertEquals(9088, agent.getProfile().getDefaultPort());
        Assertions.assertTrue(agent.getProfile().getSkipExecutionContext());
    }

    @Test
    void buildsGbase8sJdbcUrlWithExplicitServerAndLocaleParameters() {
        String url = Gbase8sAgent.buildUrl(
            new ConnectParams(
                "172.26.128.159",
                20013,
                "testdb",
                "",
                "",
                "GBASEDBTSERVER=gbase01;CLIENT_LOCALE=zh_cn.utf8;DB_LOCALE=zh_cn.utf8",
                "",
                false
            )
        );

        Assertions.assertEquals(
            "jdbc:gbasedbt-sqli://172.26.128.159:20013/testdb:GBASEDBTSERVER=gbase01;CLIENT_LOCALE=zh_cn.utf8;DB_LOCALE=zh_cn.utf8",
            url
        );
    }

    @Test
    void fallsBackToHostAsGbaseServerWhenNoExplicitServerIsConfigured() {
        String url = Gbase8sAgent.buildUrl(
            new ConnectParams(
                "gbase-host",
                9088,
                "sysmaster",
                "",
                "",
                "",
                "",
                false
            )
        );

        Assertions.assertEquals(
            "jdbc:gbasedbt-sqli://gbase-host:9088/sysmaster:GBASEDBTSERVER=gbase-host",
            url
        );
    }

    @Test
    void fallsBackToGbaseServerNameWhenHostIsAnIpAddress() {
        String url = Gbase8sAgent.buildUrl(
            new ConnectParams(
                "172.26.128.159",
                0,
                "sysmaster",
                "",
                "",
                "",
                "",
                false
            )
        );

        Assertions.assertEquals(
            "jdbc:gbasedbt-sqli://172.26.128.159:9088/sysmaster:GBASEDBTSERVER=gbase8s",
            url
        );
    }

    @Test
    void usesConnectionStringWhenConfigured() {
        String url = Gbase8sAgent.buildUrl(
            new ConnectParams(
                "ignored",
                0,
                "",
                "",
                "",
                "",
                "jdbc:gbasedbt-sqli://db.example.com:20013/app:GBASEDBTSERVER=gbase01",
                false
            )
        );

        Assertions.assertEquals("jdbc:gbasedbt-sqli://db.example.com:20013/app:GBASEDBTSERVER=gbase01", url);
    }
}
