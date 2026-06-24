package com.dbx.agent.dameng;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;

class DamengAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new DamengAgent();
    }

    @Override
    protected String resultSetSql() {
        return "CALL SP_SAMPLE()";
    }
}
