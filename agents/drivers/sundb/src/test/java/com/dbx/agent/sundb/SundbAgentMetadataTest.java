package com.dbx.agent.sundb;

import com.dbx.agent.test.TestSupport;
import com.dbx.agent.test.JdbcAgentFake;
import com.dbx.agent.test.JdbcMetadataSqlFake;
import java.util.Collections;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class SundbAgentMetadataTest {
    @Test
    void quotesSchemaAndTableIdentifiersInIndexMetadataSql() {
        SundbAgent agent = new SundbAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.listIndexes("bad`schema", "bad`table");

        Assertions.assertEquals(
            Collections.singletonList("SHOW INDEX FROM `bad``table` FROM `bad``schema`"),
            JdbcMetadataSqlFake.statements
        );
    }
}
