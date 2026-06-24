package com.dbx.agent.gbase8a;

import com.dbx.agent.ConfiguredJdbcAgent;
import com.dbx.agent.JdbcAgentProfile;
import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.TableInfo;

import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

public final class Gbase8aAgent extends ConfiguredJdbcAgent {
    public static final JdbcAgentProfile GBASE8A_PROFILE = new JdbcAgentProfile(
        "com.gbase.jdbc.Driver",
        "jdbc:gbase://{host}:{port}/{database}?useSSL=false",
        5258,
        false,
        java.util.Collections.emptySet(),
        java.util.Arrays.asList("TABLE", "VIEW", "BASE TABLE"),
        "`",
        "USE",
        true,
        false,
        false,
        false
    );

    public Gbase8aAgent() {
        super(GBASE8A_PROFILE);
    }

    @Override
    public List<TableInfo> listTables(String schema) {
        return unchecked(() -> {
            List<TableInfo> result = new ArrayList<>();
            String sql;
            if (schema != null && !schema.trim().isEmpty()) {
                sql = "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME";
            } else {
                sql = "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA NOT IN ('information_schema', 'performance_schema', 'gclusterdb', 'gctmpdb') ORDER BY TABLE_SCHEMA, TABLE_NAME";
            }
            try (PreparedStatement stmt = requireConnection().prepareStatement(sql)) {
                if (schema != null && !schema.trim().isEmpty()) {
                    stmt.setString(1, schema);
                }
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String tableType = rs.getString("TABLE_TYPE");
                        if ("BASE TABLE".equals(tableType)) {
                            tableType = "TABLE";
                        }
                        result.add(new TableInfo(rs.getString("TABLE_NAME"), tableType, null));
                    }
                }
            }
            result.sort(Comparator.comparing(TableInfo::getName));
            return result;
        });
    }

    public static void main(String[] args) {
        new JsonRpcServer(new Gbase8aAgent()).run();
    }
}
