package com.dbx.agent.opengauss;

import com.dbx.agent.ConnectParams;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class OpenGaussAgentTest {
    @Test
    void buildsOpenGaussUrl() {
        ConnectParams params = new ConnectParams();
        params.setHost("db.example.com");
        params.setPort(15432);
        params.setDatabase("postgres");

        assertEquals(
            "jdbc:opengauss://db.example.com:15432/postgres",
            OpenGaussAgent.OPENGAUSS_PROFILE.buildUrl(params)
        );
    }
}
