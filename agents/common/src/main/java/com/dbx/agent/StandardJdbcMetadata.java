package com.dbx.agent;

import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.ResultSet;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;

public final class StandardJdbcMetadata {
    public static final StandardJdbcMetadata INSTANCE = new StandardJdbcMetadata();

    private StandardJdbcMetadata() {
    }

    public List<DatabaseInfo> listDatabases(Connection conn, String configuredDatabase) {
        return unchecked(() -> {
            Set<String> names = new LinkedHashSet<>();
            try {
                try (ResultSet rs = conn.getMetaData().getCatalogs()) {
                    while (rs.next()) {
                        addNonBlank(names, rs.getString("TABLE_CAT"));
                    }
                }
            } catch (Exception ignored) {
            }
            addNonBlank(names, configuredDatabase);
            addNonBlank(names, conn.getCatalog());

            List<DatabaseInfo> result = new ArrayList<>();
            for (String name : names) {
                result.add(new DatabaseInfo(name));
            }
            return result;
        });
    }

    public List<String> listSchemas(Connection conn, JdbcAgentProfile profile) {
        return unchecked(() -> {
            Set<String> names = new LinkedHashSet<>();
            DatabaseMetaData meta = conn.getMetaData();
            try {
                appendSchemas(names, meta.getSchemas(null, null));
            } catch (Exception | AbstractMethodError first) {
                try {
                    appendSchemas(names, meta.getSchemas());
                } catch (Exception | AbstractMethodError ignored) {
                }
            }
            try {
                addNonBlank(names, conn.getSchema());
            } catch (Exception | AbstractMethodError ignored) {
            }

            List<String> result = new ArrayList<>();
            for (String name : names) {
                if (!profile.getExcludedSchemas().contains(name.toUpperCase(Locale.ROOT))) {
                    result.add(name);
                }
            }
            Collections.sort(result);
            return result;
        });
    }

    public List<TableInfo> listTables(Connection conn, JdbcAgentProfile profile, String configuredDatabase, String schema) {
        return unchecked(() -> {
            DatabaseMetaData meta = conn.getMetaData();
            List<TableInfo> result = new ArrayList<>();
            appendTables(result, meta, profile, null, schema);
            if (result.isEmpty() && profile.getCatalogFallbackEnabled() && !configuredDatabase.trim().isEmpty()) {
                appendTables(result, meta, profile, configuredDatabase, schema);
            }
            result.sort(Comparator.comparing(TableInfo::getName));
            return result;
        });
    }

    public List<ObjectInfo> listObjects(List<TableInfo> tables, String schema) {
        List<ObjectInfo> result = new ArrayList<>();
        for (TableInfo table : tables) {
            result.add(new ObjectInfo(table.getName(), table.getTable_type(), schema, table.getComment()));
        }
        return result;
    }

    public List<ColumnInfo> getColumns(Connection conn, JdbcAgentProfile profile, String configuredDatabase, String schema, String table) {
        return unchecked(() -> {
            DatabaseMetaData meta = conn.getMetaData();
            Set<String> primaryKeys = primaryKeys(meta, null, schema, table);
            List<ColumnInfo> result = new ArrayList<>();
            appendColumns(result, meta, null, schema, table, primaryKeys);
            if (result.isEmpty() && profile.getCatalogFallbackEnabled() && !configuredDatabase.trim().isEmpty()) {
                Set<String> fallbackPrimaryKeys = primaryKeys(meta, configuredDatabase, schema, table);
                appendColumns(result, meta, configuredDatabase, schema, table, fallbackPrimaryKeys);
            }
            return result;
        });
    }

    public List<IndexInfo> listIndexes(Connection conn, String schema, String table) {
        return unchecked(() -> {
            DatabaseMetaData meta = conn.getMetaData();
            Map<String, List<IndexColumn>> indexes = new LinkedHashMap<>();
            Map<String, Boolean> unique = new LinkedHashMap<>();
            try (ResultSet rs = meta.getIndexInfo(null, blankToNull(schema), table, false, false)) {
                while (rs.next()) {
                    String name = rs.getString("INDEX_NAME");
                    String column = rs.getString("COLUMN_NAME");
                    if (name == null || column == null) {
                        continue;
                    }
                    indexes.computeIfAbsent(name, ignored -> new ArrayList<>()).add(
                        new IndexColumn(rs.getShort("ORDINAL_POSITION"), column)
                    );
                    unique.put(name, !rs.getBoolean("NON_UNIQUE"));
                }
            }

            List<IndexInfo> result = new ArrayList<>();
            for (Map.Entry<String, List<IndexColumn>> entry : indexes.entrySet()) {
                List<IndexColumn> orderedColumns = entry.getValue();
                orderedColumns.sort(Comparator.comparingInt(IndexColumn::getOrdinal));
                List<String> columns = new ArrayList<>();
                for (IndexColumn column : orderedColumns) {
                    columns.add(column.getName());
                }
                String name = entry.getKey();
                result.add(new IndexInfo(
                    name,
                    columns,
                    Boolean.TRUE.equals(unique.get(name)),
                    "PRIMARY".equalsIgnoreCase(name),
                    null,
                    null,
                    null,
                    null
                ));
            }
            return result;
        });
    }

    public List<ForeignKeyInfo> listForeignKeys(Connection conn, String schema, String table) {
        return unchecked(() -> {
            DatabaseMetaData meta = conn.getMetaData();
            List<ForeignKeyInfo> result = new ArrayList<>();
            try (ResultSet rs = meta.getImportedKeys(null, blankToNull(schema), table)) {
                while (rs.next()) {
                    String name = rs.getString("FK_NAME");
                    result.add(new ForeignKeyInfo(
                        name == null ? "" : name,
                        rs.getString("FKCOLUMN_NAME"),
                        rs.getString("PKTABLE_NAME"),
                        rs.getString("PKCOLUMN_NAME")
                    ));
                }
            }
            return result;
        });
    }

    public List<TriggerInfo> listTriggers(String schema, String table) {
        return Collections.emptyList();
    }

    private static String[] getDriverTableTypes(DatabaseMetaData meta, JdbcAgentProfile profile) throws Exception {
        Set<String> configuredTypes = new LinkedHashSet<>();
        for (String type : profile.getTableTypes()) {
            if (type != null) {
                configuredTypes.add(type.toUpperCase(Locale.ROOT));
            }
        }
        try (ResultSet rs = meta.getTableTypes()) {
            List<String> types = new ArrayList<>();
            while (rs.next()) {
                String type = rs.getString("TABLE_TYPE");
                if (type != null && configuredTypes.contains(type.toUpperCase(Locale.ROOT))) {
                    types.add(type);
                }
            }
            if (!types.isEmpty()) {
                return types.toArray(new String[0]);
            }
        } catch (Exception ignored) {
        }
        return profile.getTableTypes().toArray(new String[profile.getTableTypes().size()]);
    }

    public static String getIdentifierQuote(Connection conn, JdbcAgentProfile profile) {
        try {
            String quote = conn.getMetaData().getIdentifierQuoteString();
            if (quote != null && !quote.trim().isEmpty()) {
                return quote.trim();
            }
        } catch (Exception ignored) {
        }
        return profile.getIdentifierQuote();
    }

    public static String quoteIdentifier(String identifier, String quote) {
        return quote + identifier.replace(quote, quote + quote) + quote;
    }

    public static String schemaSwitchSql(Connection conn, JdbcAgentProfile profile, String schema) {
        if (profile.getSkipExecutionContext()) {
            return profile.schemaSwitchSql(schema);
        }
        String quote = getIdentifierQuote(conn, profile);
        return profile.schemaSwitchSql(schema, quote);
    }

    private void appendTables(List<TableInfo> result, DatabaseMetaData meta, JdbcAgentProfile profile, String catalog, String schema) throws Exception {
        String[] tableTypes = getDriverTableTypes(meta, profile);
        try (ResultSet rs = meta.getTables(catalog, blankToNull(schema), "%", tableTypes)) {
            while (rs.next()) {
                result.add(new TableInfo(
                    rs.getString("TABLE_NAME"),
                    normalizeTableType(rs.getString("TABLE_TYPE")),
                    rs.getString("REMARKS")
                ));
            }
        }
    }

    private Set<String> primaryKeys(DatabaseMetaData meta, String catalog, String schema, String table) {
        Set<String> keys = new LinkedHashSet<>();
        try (ResultSet rs = meta.getPrimaryKeys(catalog, blankToNull(schema), table)) {
            while (rs.next()) {
                String name = rs.getString("COLUMN_NAME");
                if (name != null) {
                    keys.add(name);
                }
            }
        } catch (Exception ignored) {
        }
        return keys;
    }

    private static void appendSchemas(Set<String> names, ResultSet rs) throws Exception {
        try (ResultSet closeable = rs) {
            while (closeable.next()) {
                addNonBlank(names, closeable.getString("TABLE_SCHEM"));
            }
        }
    }

    private void appendColumns(
        List<ColumnInfo> result,
        DatabaseMetaData meta,
        String catalog,
        String schema,
        String table,
        Set<String> primaryKeys
    ) throws Exception {
        try (ResultSet rs = meta.getColumns(catalog, blankToNull(schema), table, "%")) {
            while (rs.next()) {
                String name = rs.getString("COLUMN_NAME");
                result.add(new ColumnInfo(
                    name,
                    rs.getString("TYPE_NAME"),
                    rs.getInt("NULLABLE") != DatabaseMetaData.columnNoNulls,
                    rs.getString("COLUMN_DEF"),
                    primaryKeys.contains(name),
                    null,
                    rs.getString("REMARKS"),
                    intOrNull(rs, "COLUMN_SIZE"),
                    intOrNull(rs, "DECIMAL_DIGITS"),
                    characterLength(rs)
                ));
            }
        }
    }

    private static String normalizeTableType(String type) {
        if (type == null || type.trim().isEmpty()) {
            return "TABLE";
        }
        String normalized = type.toUpperCase(Locale.ROOT);
        if ("BASE TABLE".equals(normalized) || "COLUMN TABLE".equals(normalized) || "ROW TABLE".equals(normalized)) {
            return "TABLE";
        }
        return type;
    }

    private static Integer intOrNull(ResultSet rs, String column) throws Exception {
        Object value = rs.getObject(column);
        return value instanceof Number ? ((Number) value).intValue() : null;
    }

    private static Integer characterLength(ResultSet rs) throws Exception {
        String typeName = rs.getString("TYPE_NAME");
        if (typeName == null) {
            return null;
        }
        String normalized = typeName.toLowerCase(Locale.ROOT);
        if (!normalized.contains("char") && !normalized.contains("text")) {
            return null;
        }
        return intOrNull(rs, "COLUMN_SIZE");
    }

    private static String blankToNull(String value) {
        return value == null || value.trim().isEmpty() ? null : value;
    }

    private static void addNonBlank(Set<String> values, String value) {
        if (value != null && !value.trim().isEmpty()) {
            values.add(value);
        }
    }

    private static <T> T unchecked(ThrowingSupplier<T> supplier) {
        try {
            return supplier.get();
        } catch (RuntimeException e) {
            throw e;
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }

    private static final class IndexColumn {
        private final int ordinal;
        private final String name;

        private IndexColumn(int ordinal, String name) {
            this.ordinal = ordinal;
            this.name = name;
        }

        private int getOrdinal() {
            return ordinal;
        }

        private String getName() {
            return name;
        }
    }

    private interface ThrowingSupplier<T> {
        T get() throws Exception;
    }
}
