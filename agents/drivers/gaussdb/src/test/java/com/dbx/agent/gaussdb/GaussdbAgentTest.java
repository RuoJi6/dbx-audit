package com.dbx.agent.gaussdb;

import com.dbx.agent.BaseDatabaseAgent;
import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;
import com.dbx.agent.test.JdbcMetadataSqlFake;
import com.dbx.agent.test.TestSupport;
import java.util.Arrays;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class GaussdbAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new GaussdbAgent();
    }

    @Override
    protected String resultSetSql() {
        return "CALL sample_proc()";
    }

    @Test
    void agentExtendsBaseDatabaseAgent() {
        Assertions.assertTrue(BaseDatabaseAgent.class.isAssignableFrom(GaussdbAgent.class));
    }

    @Test
    void readsViewSourceWithQuotedRegclassParameter() {
        GaussdbAgent agent = new GaussdbAgent();
        TestSupport.setPrivateConnection(agent, JdbcMetadataSqlFake.connection());

        agent.getObjectSource("bad\"schema", "view's name", "VIEW");

        Assertions.assertEquals(
            Arrays.asList(
                "SELECT pg_get_viewdef(to_regclass(?), true)",
                "param:1=\"bad\"\"schema\".\"view's name\""
            ),
            JdbcMetadataSqlFake.statements
        );
    }
}
