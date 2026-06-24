package com.dbx.agent.hive;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;

class HiveAgentExecutionTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new HiveAgent();
    }

    @Override
    protected String resultSetSql() {
        return "MSCK REPAIR TABLE sample";
    }
}
