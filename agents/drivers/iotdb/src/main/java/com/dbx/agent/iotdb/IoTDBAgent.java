package com.dbx.agent.iotdb;

import com.dbx.agent.ConfiguredJdbcAgent;
import com.dbx.agent.JdbcAgentProfile;
import com.dbx.agent.JsonRpcServer;

public final class IoTDBAgent extends ConfiguredJdbcAgent {
    public static final JdbcAgentProfile IOTDB_PROFILE = new JdbcAgentProfile(
        "org.apache.iotdb.jdbc.IoTDBDriver",
        "jdbc:iotdb://{host}:{port}/",
        6667,
        true
    );

    public IoTDBAgent() {
        super(IOTDB_PROFILE);
    }

    public static void main(String[] args) {
        new JsonRpcServer(new IoTDBAgent()).run();
    }
}
