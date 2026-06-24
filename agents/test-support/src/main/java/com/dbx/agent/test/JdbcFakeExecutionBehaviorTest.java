package com.dbx.agent.test;

import com.dbx.agent.DatabaseAgent;
import com.dbx.agent.ExecuteQueryOptions;
import com.dbx.agent.QueryResult;
import org.junit.jupiter.api.Test;

import java.util.Collections;

import static org.junit.jupiter.api.Assertions.assertEquals;

public abstract class JdbcFakeExecutionBehaviorTest {
    protected abstract DatabaseAgent createAgent();

    protected abstract String resultSetSql();

    @Test
    void executesNonSelectStatementsThatReturnResultSets() {
        DatabaseAgent agent = createAgent();
        TestSupport.setPrivateConnection(agent, JdbcAgentFake.connection());

        QueryResult result = agent.executeQuery(resultSetSql(), null, new ExecuteQueryOptions());

        assertEquals(Collections.singletonList("VALUE"), result.getColumns());
        assertEquals(Collections.singletonList(Collections.singletonList("row-value")), result.getRows());
        assertEquals(asList("setMaxRows:10001", "execute"), JdbcAgentFake.calls);
    }

    @Test
    void appliesExecutionRowAndFetchLimitsToJdbcStatements() {
        DatabaseAgent agent = createAgent();
        TestSupport.setPrivateConnection(agent, JdbcAgentFake.connection());

        QueryResult result = agent.executeQuery(resultSetSql(), null, new ExecuteQueryOptions(1, 25));

        assertEquals(Collections.singletonList(Collections.singletonList("row-value")), result.getRows());
        assertEquals(asList("setMaxRows:2", "setFetchSize:25", "execute"), JdbcAgentFake.calls);
    }

    private static java.util.List<String> asList(String first, String second) {
        return java.util.Arrays.asList(first, second);
    }

    private static java.util.List<String> asList(String first, String second, String third) {
        return java.util.Arrays.asList(first, second, third);
    }
}
