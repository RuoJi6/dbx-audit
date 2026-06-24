package com.dbx.agent.goldendb;

import com.dbx.agent.test.TestSupport;
import com.dbx.agent.test.JdbcAgentFake;
import com.dbx.agent.test.JdbcMetadataSqlFake;
import java.sql.Connection;
import java.util.List;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class GoldendbAgentMetadataTest {
    @Test
    void quotesSchemaAndTableIdentifiersInIndexMetadataSql() {
        GoldendbAgent agent = new GoldendbAgent();
        Connection fake = JdbcMetadataSqlFake.connection();
        TestSupport.setPrivateConnection(agent, fake);

        agent.listIndexes("bad`schema", "bad`table");

        Assertions.assertEquals(
            List.of("SHOW INDEX FROM `bad``table` FROM `bad``schema`"),
            JdbcMetadataSqlFake.statements
        );
    }
}
