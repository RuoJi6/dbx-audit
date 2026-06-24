package com.dbx.agent.snowflake;

import com.dbx.agent.AbstractJdbcAgent;
import com.dbx.agent.ColumnInfo;
import com.dbx.agent.ConnectParams;
import com.dbx.agent.DatabaseInfo;
import com.dbx.agent.ForeignKeyInfo;
import com.dbx.agent.IndexInfo;
import com.dbx.agent.JdbcIdentifiers;
import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.ObjectInfo;
import com.dbx.agent.ObjectSource;
import com.dbx.agent.TableInfo;
import com.dbx.agent.TriggerInfo;
import java.sql.Connection;
import java.sql.ResultSet;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;

public final class SnowflakeAgent extends AbstractJdbcAgent {

    @Override
    protected String driverClass() {
        return "net.snowflake.client.jdbc.SnowflakeDriver";
    }

    @Override
    protected String buildJdbcUrl(ConnectParams params) {
        return buildUrl(params);
    }

    @Override
    public List<DatabaseInfo> listDatabases() {
        return unchecked(() -> {
            List<DatabaseInfo> result = new ArrayList<>();
            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery("SHOW DATABASES")) {
                while (rs.next()) {
                    result.add(new DatabaseInfo(rs.getString("name")));
                }
            }
            result.sort(Comparator.comparing(DatabaseInfo::getName));
            return result;
        });
    }

    @Override
    public List<String> listSchemas() {
        return unchecked(() -> {
            List<String> result = new ArrayList<>();
            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery("SHOW SCHEMAS")) {
                while (rs.next()) {
                    result.add(rs.getString("name"));
                }
            }
            Collections.sort(result);
            return result;
        });
    }

    @Override
    public List<TableInfo> listTables(String schema) {
        return unchecked(() -> {
            List<TableInfo> result = new ArrayList<>();
            String sql = "SELECT TABLE_NAME, TABLE_TYPE " +
                "FROM INFORMATION_SCHEMA.TABLES " +
                "WHERE TABLE_SCHEMA = ? " +
                "ORDER BY TABLE_NAME";
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new TableInfo(
                            rs.getString("TABLE_NAME"),
                            normalizeTableType(rs.getString("TABLE_TYPE")),
                            null
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<ObjectInfo> listObjects(String schema) {
        return unchecked(() -> {
            List<ObjectInfo> result = new ArrayList<>();
            for (TableInfo table : listTables(schema)) {
                result.add(new ObjectInfo(table.getName(), table.getTable_type(), schema, table.getComment()));
            }

            String sql = "SELECT PROCEDURE_NAME, 'PROCEDURE' " +
                "FROM INFORMATION_SCHEMA.PROCEDURES " +
                "WHERE PROCEDURE_SCHEMA = ? " +
                "ORDER BY PROCEDURE_NAME";
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ObjectInfo(rs.getString(1), rs.getString(2), schema, null));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public ObjectSource getObjectSource(String schema, String name, String objectType) {
        return unchecked(() -> {
            String ddlType = objectType.toUpperCase(Locale.ROOT);
            String sql = "SELECT GET_DDL('" + ddlType + "', '\"" + schema + "\".\"" + name + "\"')";
            String source = "";
            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery(sql)) {
                if (rs.next()) {
                    String value = rs.getString(1);
                    source = value == null ? "" : value;
                }
            }
            return new ObjectSource(name, objectType, schema, source);
        });
    }

    @Override
    public List<ColumnInfo> getColumns(String schema, String table) {
        return unchecked(() -> {
            Connection conn = requireConnected();
            List<String> primaryKeys = new ArrayList<>();
            try (java.sql.Statement stmt = conn.createStatement()) {
                String qualifiedTable = JdbcIdentifiers.INSTANCE.doubleQuote(schema) + "." + JdbcIdentifiers.INSTANCE.doubleQuote(table);
                try (ResultSet rs = stmt.executeQuery("SHOW PRIMARY KEYS IN TABLE " + qualifiedTable)) {
                    while (rs.next()) {
                        primaryKeys.add(rs.getString("column_name"));
                    }
                }
            } catch (Exception ignored) {
                // Snowflake may not return primary key metadata for every object.
            }

            List<ColumnInfo> result = new ArrayList<>();
            String sql = "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, " +
                "NUMERIC_PRECISION, NUMERIC_SCALE, CHARACTER_MAXIMUM_LENGTH, COMMENT " +
                "FROM INFORMATION_SCHEMA.COLUMNS " +
                "WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? " +
                "ORDER BY ORDINAL_POSITION";
            try (java.sql.PreparedStatement stmt = conn.prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String colName = rs.getString("COLUMN_NAME");
                        result.add(new ColumnInfo(
                            colName,
                            rs.getString("DATA_TYPE"),
                            "YES".equals(rs.getString("IS_NULLABLE")),
                            rs.getString("COLUMN_DEFAULT"),
                            primaryKeys.contains(colName),
                            null,
                            blankToNull(rs.getString("COMMENT")),
                            intOrNull(rs, "NUMERIC_PRECISION"),
                            intOrNull(rs, "NUMERIC_SCALE"),
                            intOrNull(rs, "CHARACTER_MAXIMUM_LENGTH")
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<IndexInfo> listIndexes(String schema, String table) {
        return Collections.emptyList();
    }

    @Override
    public List<ForeignKeyInfo> listForeignKeys(String schema, String table) {
        return unchecked(() -> {
            try (java.sql.Statement stmt = requireConnected().createStatement()) {
                String qualifiedTable = JdbcIdentifiers.INSTANCE.doubleQuote(schema) + "." + JdbcIdentifiers.INSTANCE.doubleQuote(table);
                try (ResultSet rs = stmt.executeQuery("SHOW IMPORTED KEYS IN TABLE " + qualifiedTable)) {
                    List<ForeignKeyInfo> result = new ArrayList<>();
                    while (rs.next()) {
                        result.add(new ForeignKeyInfo(
                            rs.getString("fk_name"),
                            rs.getString("fk_column_name"),
                            rs.getString("pk_table_name"),
                            rs.getString("pk_column_name")
                        ));
                    }
                    return result;
                }
            } catch (Exception ignored) {
                return Collections.emptyList();
            }
        });
    }

    @Override
    public List<TriggerInfo> listTriggers(String schema, String table) {
        return Collections.emptyList();
    }

    @Override
    public String setSchemaSQL(String schema) {
        return "USE SCHEMA " + JdbcIdentifiers.INSTANCE.doubleQuote(schema);
    }

    @Override
    protected Object resultValue(ResultSet rs, int index, int sqlType) {
        return unchecked(() -> {
            Object value = rs.getObject(index);
            return rs.wasNull() ? null : value == null ? null : value.toString();
        });
    }

    private static String buildUrl(ConnectParams params) {
        return "jdbc:snowflake://" + params.getHost() + ":" + params.getPort() + "/?db=" + params.getDatabase();
    }

    private static String normalizeTableType(String tableType) {
        return "BASE TABLE".equals(tableType) ? "TABLE" : tableType;
    }

    private static String blankToNull(String value) {
        return value == null || value.isEmpty() ? null : value;
    }

    private static Integer intOrNull(ResultSet rs, String column) throws java.sql.SQLException {
        Object value = rs.getObject(column);
        return value instanceof Number ? ((Number) value).intValue() : null;
    }

    public static void main(String[] args) {
        new JsonRpcServer(new SnowflakeAgent()).run();
    }
}
