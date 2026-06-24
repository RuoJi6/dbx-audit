package com.dbx.agent.trino;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;

class TrinoAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new TrinoAgent();
    }

    @Override
    protected String resultSetSql() {
        return "CALL system.runtime.nodes()";
    }
}
