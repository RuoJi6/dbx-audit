package com.dbx.agent.opengauss;

import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.PostgresLikeAgent;
import com.dbx.agent.PostgresLikeAgentProfile;

public final class OpenGaussAgent extends PostgresLikeAgent {
    public static final PostgresLikeAgentProfile OPENGAUSS_PROFILE = new PostgresLikeAgentProfile(
        "org.opengauss.Driver",
        "jdbc:opengauss://{host}:{port}/{database}"
    );

    public OpenGaussAgent() {
        super(OPENGAUSS_PROFILE);
    }

    public static void main(String[] args) {
        new JsonRpcServer(new OpenGaussAgent()).run();
    }
}
