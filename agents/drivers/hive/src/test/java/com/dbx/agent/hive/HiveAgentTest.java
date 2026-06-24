package com.dbx.agent.hive;

import org.junit.jupiter.api.Test;

class HiveAgentTest {
    @Test
    void hiveJdbcStandaloneRuntimeClassesAreAvailable() throws ClassNotFoundException {
        Class.forName("org.apache.hive.jdbc.HiveDriver");
        Class.forName("org.apache.hive.org.apache.thrift.protocol.TProtocol");
    }
}
