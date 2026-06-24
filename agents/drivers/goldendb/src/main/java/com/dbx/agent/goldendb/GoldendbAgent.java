package com.dbx.agent.goldendb;

import com.dbx.agent.BaseDatabaseAgent;
import com.dbx.agent.ColumnInfo;
import com.dbx.agent.ConnectParams;
import com.dbx.agent.DatabaseInfo;
import com.dbx.agent.ExecuteQueryOptions;
import com.dbx.agent.ForeignKeyInfo;
import com.dbx.agent.IndexInfo;
import com.dbx.agent.JdbcExecutor;
import com.dbx.agent.JdbcIdentifiers;
import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.ObjectInfo;
import com.dbx.agent.ObjectSource;
import com.dbx.agent.QueryResult;
import com.dbx.agent.TableInfo;
import com.dbx.agent.TriggerInfo;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Types;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.HashSet;

public final class GoldendbAgent extends BaseDatabaseAgent {
    private Connection connection;

    @Override
    public Connection getConnection() {
        return connection;
    }

    @Override
    public void connect(ConnectParams params) {
        uncheckedVoid(() -> {
            Class.forName("com.mysql.cj.jdbc.Driver");
            connection = DriverManager.getConnection(buildUrl(params), params.getUsername(), params.getPassword());
        });
    }

    @Override
    public boolean testConnection(ConnectParams params) {
        return unchecked(() -> {
            Class.forName("com.mysql.cj.jdbc.Driver");
            try (Connection conn = DriverManager.getConnection(buildUrl(params), params.getUsername(), params.getPassword())) {
                return conn.isValid(5);
            }
        });
    }

    @Override
    public List<DatabaseInfo> listDatabases() {
        return unchecked(() -> {
            List<DatabaseInfo> result = new ArrayList<>();
            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery("SHOW DATABASES")) {
                while (rs.next()) {
                    result.add(new DatabaseInfo(rs.getString(1)));
                }
            }
            return result;
        });
    }

    @Override
    public List<String> listSchemas() {
        List<String> result = new ArrayList<>();
        for (DatabaseInfo database : listDatabases()) {
            result.add(database.getName());
        }
        return result;
    }

    @Override
    public List<TableInfo> listTables(String schema) {
        return unchecked(() -> {
            List<TableInfo> result = new ArrayList<>();
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                "SELECT TABLE_NAME, TABLE_TYPE, TABLE_COMMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME"
            )) {
                stmt.setString(1, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String tableType = rs.getString("TABLE_TYPE");
                        if ("BASE TABLE".equals(tableType)) {
                            tableType = "TABLE";
                        }
                        result.add(new TableInfo(rs.getString("TABLE_NAME"), tableType, emptyToNull(rs.getString("TABLE_COMMENT"))));
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

            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                "SELECT ROUTINE_NAME, ROUTINE_TYPE FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME"
            )) {
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
            String quotedName = JdbcIdentifiers.INSTANCE.backtick(name);
            String sql = switch (objectType.toUpperCase(Locale.ROOT)) {
                case "VIEW" -> "SHOW CREATE VIEW " + quotedName;
                case "PROCEDURE" -> "SHOW CREATE PROCEDURE " + quotedName;
                case "FUNCTION" -> "SHOW CREATE FUNCTION " + quotedName;
                default -> throw new IllegalArgumentException("Unsupported object type: " + objectType);
            };

            String source = "";
            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery(sql)) {
                if (rs.next()) {
                    int sourceIndex = "VIEW".equals(objectType.toUpperCase(Locale.ROOT)) ? 2 : 3;
                    String value = rs.getString(sourceIndex);
                    source = value == null ? "" : value;
                }
            }
            return new ObjectSource(name, objectType, schema, source);
        });
    }

    @Override
    public List<ColumnInfo> getColumns(String schema, String table) {
        return unchecked(() -> {
            Set<String> primaryKeys = new HashSet<>();
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                """
                SELECT COLUMN_NAME
                FROM information_schema.KEY_COLUMN_USAGE
                WHERE TABLE_SCHEMA = ?
                    AND TABLE_NAME = ?
                    AND CONSTRAINT_NAME = 'PRIMARY'
                """
            )) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        primaryKeys.add(rs.getString("COLUMN_NAME"));
                    }
                }
            }

            List<ColumnInfo> result = new ArrayList<>();
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                """
                SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, COLUMN_COMMENT,
                       NUMERIC_PRECISION, NUMERIC_SCALE, CHARACTER_MAXIMUM_LENGTH
                FROM information_schema.COLUMNS
                WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
                ORDER BY ORDINAL_POSITION
                """
            )) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String columnName = rs.getString("COLUMN_NAME");
                        result.add(new ColumnInfo(
                            columnName,
                            rs.getString("COLUMN_TYPE"),
                            "YES".equals(rs.getString("IS_NULLABLE")),
                            rs.getString("COLUMN_DEFAULT"),
                            primaryKeys.contains(columnName),
                            rs.getString("EXTRA"),
                            emptyToNull(rs.getString("COLUMN_COMMENT")),
                            integerOrNull(rs, "NUMERIC_PRECISION"),
                            integerOrNull(rs, "NUMERIC_SCALE"),
                            numberToIntOrNull(rs, "CHARACTER_MAXIMUM_LENGTH")
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<IndexInfo> listIndexes(String schema, String table) {
        return unchecked(() -> {
            Map<String, List<IndexColumn>> indexMap = new TreeMap<>();
            Map<String, Boolean> uniqueMap = new HashMap<>();
            Map<String, String> typeMap = new HashMap<>();
            String quotedTable = JdbcIdentifiers.INSTANCE.backtick(table);
            String quotedSchema = JdbcIdentifiers.INSTANCE.backtick(schema);

            try (java.sql.Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery("SHOW INDEX FROM " + quotedTable + " FROM " + quotedSchema)) {
                while (rs.next()) {
                    String indexName = rs.getString("Key_name");
                    String columnName = rs.getString("Column_name");
                    int seqInIndex = rs.getInt("Seq_in_index");
                    int nonUnique = rs.getInt("Non_unique");
                    String indexType = rs.getString("Index_type");

                    indexMap.computeIfAbsent(indexName, ignored -> new ArrayList<>())
                        .add(new IndexColumn(seqInIndex, columnName));
                    uniqueMap.put(indexName, nonUnique == 0);
                    typeMap.put(indexName, indexType);
                }
            }

            List<IndexInfo> result = new ArrayList<>();
            for (Map.Entry<String, List<IndexColumn>> entry : indexMap.entrySet()) {
                List<IndexColumn> columns = entry.getValue();
                columns.sort((left, right) -> Integer.compare(left.ordinal, right.ordinal));
                List<String> columnNames = new ArrayList<>();
                for (IndexColumn column : columns) {
                    columnNames.add(column.name);
                }

                String indexName = entry.getKey();
                result.add(new IndexInfo(
                    indexName,
                    columnNames,
                    Boolean.TRUE.equals(uniqueMap.get(indexName)),
                    "PRIMARY".equals(indexName),
                    null,
                    typeMap.get(indexName),
                    null,
                    null
                ));
            }
            return result;
        });
    }

    @Override
    public List<ForeignKeyInfo> listForeignKeys(String schema, String table) {
        return unchecked(() -> {
            List<ForeignKeyInfo> result = new ArrayList<>();
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                """
                SELECT CONSTRAINT_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME
                FROM information_schema.KEY_COLUMN_USAGE
                WHERE TABLE_SCHEMA = ?
                    AND TABLE_NAME = ?
                    AND REFERENCED_TABLE_NAME IS NOT NULL
                ORDER BY CONSTRAINT_NAME
                """
            )) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ForeignKeyInfo(
                            rs.getString("CONSTRAINT_NAME"),
                            rs.getString("COLUMN_NAME"),
                            rs.getString("REFERENCED_TABLE_NAME"),
                            rs.getString("REFERENCED_COLUMN_NAME")
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<TriggerInfo> listTriggers(String schema, String table) {
        return unchecked(() -> {
            List<TriggerInfo> result = new ArrayList<>();
            try (java.sql.PreparedStatement stmt = requireConnected().prepareStatement(
                """
                SELECT TRIGGER_NAME, EVENT_MANIPULATION, ACTION_TIMING
                FROM information_schema.TRIGGERS
                WHERE TRIGGER_SCHEMA = ? AND EVENT_OBJECT_TABLE = ?
                ORDER BY TRIGGER_NAME
                """
            )) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new TriggerInfo(
                            rs.getString("TRIGGER_NAME"),
                            rs.getString("EVENT_MANIPULATION"),
                            rs.getString("ACTION_TIMING")
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public QueryResult executeQuery(String sql, String schema, ExecuteQueryOptions options) {
        return JdbcExecutor.INSTANCE.execute(
            requireConnected(),
            sql,
            schema,
            this::setSchemaSQL,
            options.getMaxRows(),
            options.getFetchSize(),
            options.getTimeoutSecs(),
            this::resultValue
        );
    }

    @Override
    public String setSchemaSQL(String schema) {
        return "USE " + JdbcIdentifiers.INSTANCE.backtick(schema);
    }

    @Override
    public void disconnect() {
        uncheckedVoid(() -> {
            if (connection != null) {
                connection.close();
            }
            connection = null;
        });
    }

    private Object resultValue(ResultSet rs, int index, int sqlType) {
        return unchecked(() -> {
            Object value;
            switch (sqlType) {
                case Types.BIGINT:
                    value = rs.getLong(index);
                    break;
                case Types.INTEGER:
                case Types.SMALLINT:
                case Types.TINYINT:
                    value = rs.getInt(index);
                    break;
                case Types.FLOAT:
                case Types.REAL:
                    value = rs.getFloat(index);
                    break;
                case Types.DOUBLE:
                    value = rs.getDouble(index);
                    break;
                case Types.DECIMAL:
                case Types.NUMERIC:
                    value = rs.getBigDecimal(index);
                    break;
                case Types.BOOLEAN:
                case Types.BIT:
                    value = rs.getBoolean(index);
                    break;
                default:
                    value = rs.getString(index);
                    break;
            }
            return rs.wasNull() ? null : value;
        });
    }

    private static String buildUrl(ConnectParams params) {
        return "jdbc:mysql://" + params.getHost() + ":" + params.getPort() + "/" + params.getDatabase()
            + "?useSSL=false&allowPublicKeyRetrieval=true";
    }

    private static String emptyToNull(String value) {
        return value == null || value.isEmpty() ? null : value;
    }

    private static Integer integerOrNull(ResultSet rs, String column) throws Exception {
        Object value = rs.getObject(column);
        return value instanceof Integer ? (Integer) value : null;
    }

    private static Integer numberToIntOrNull(ResultSet rs, String column) throws Exception {
        Object value = rs.getObject(column);
        return value instanceof Number ? ((Number) value).intValue() : null;
    }

    private static final class IndexColumn {
        private final int ordinal;
        private final String name;

        private IndexColumn(int ordinal, String name) {
            this.ordinal = ordinal;
            this.name = name;
        }
    }

    public static void main(String[] args) {
        new JsonRpcServer(new GoldendbAgent()).run();
    }
}
