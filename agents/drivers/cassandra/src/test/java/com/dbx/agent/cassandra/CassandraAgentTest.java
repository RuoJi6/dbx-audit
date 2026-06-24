package com.dbx.agent.cassandra;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.test.JdbcFakeExecutionBehaviorTest;

class CassandraAgentTest extends JdbcFakeExecutionBehaviorTest {
    @Override
    protected DatabaseAgent createAgent() {
        return new CassandraAgent();
    }

    @Override
    protected String resultSetSql() {
        return "LIST ROLES";
    }
}
