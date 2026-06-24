package com.dbx.agent.snowflake;

import com.dbx.agent.test.TestSupport;
import com.dbx.agent.test.JdbcAgentFake;
import com.dbx.agent.test.JdbcMetadataSqlFake;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class SnowflakeAgentMetadataTest {
    @Test
    void quotesSchemaAndTableIdentifiersInKeyMetadataSql() {
        SnowflakeAgent agent = new SnowflakeAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.getColumns("bad\"schema", "bad\"table");
        agent.listForeignKeys("bad\"schema", "bad\"table");

        Assertions.assertTrue(
            JdbcMetadataSqlFake.statements.contains(
                "SHOW PRIMARY KEYS IN TABLE \"bad\"\"schema\".\"bad\"\"table\""
            )
        );
        Assertions.assertTrue(
            JdbcMetadataSqlFake.statements.contains(
                "SHOW IMPORTED KEYS IN TABLE \"bad\"\"schema\".\"bad\"\"table\""
            )
        );
    }
}
