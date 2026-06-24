package com.dbx.agent.oraclelegacy;

import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.oracle.OracleAgent;

public final class OracleLegacyAgent extends OracleAgent {
    public static void main(String[] args) {
        new JsonRpcServer(new OracleLegacyAgent()).run();
    }
}
