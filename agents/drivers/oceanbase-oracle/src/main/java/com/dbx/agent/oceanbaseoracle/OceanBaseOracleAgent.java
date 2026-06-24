package com.dbx.agent.oceanbaseoracle;

import com.dbx.agent.ColumnInfo;
import com.dbx.agent.ConfiguredJdbcAgent;
import com.dbx.agent.ConnectParams;
import com.dbx.agent.DatabaseInfo;
import com.dbx.agent.ForeignKeyInfo;
import com.dbx.agent.IndexInfo;
import com.dbx.agent.JdbcAgentProfile;
import com.dbx.agent.JdbcIdentifiers;
import com.dbx.agent.JsonRpcServer;
import com.dbx.agent.ObjectInfo;
import com.dbx.agent.TableInfo;
import com.dbx.agent.TriggerInfo;

import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;

public final class OceanBaseOracleAgent extends ConfiguredJdbcAgent {
    private static final String COMPATIBLE_OJDBC_VERSION = "compatibleOjdbcVersion";
    private static final String DEFAULT_COMPATIBLE_OJDBC_VERSION = "compatibleOjdbcVersion=8";
    private static final Set<String> SYSTEM_SCHEMAS = Set.of(
        "SYS", "SYSTEM", "SYSMAN", "DBSNMP", "SYSBACKUP", "SYSDG", "SYSKM", "OUTLN",
        "AUDSYS", "LBACSYS", "DVF", "DVSYS", "APPQOSSYS", "CTXSYS", "MDSYS", "MDDATA",
        "ORDSYS", "ORDDATA", "ORDPLUGINS", "XDB", "ANONYMOUS", "DIP", "EXFSYS",
        "GSMADMIN_INTERNAL", "GSMCATUSER", "GSMUSER", "OJVMSYS", "OLAPSYS",
        "ORACLE_OCM", "SI_INFORMTN_SCHEMA", "WMSYS", "XS$NULL", "DBSFWUSER",
        "REMOTE_SCHEDULER_AGENT", "PDBADMIN", "DGPDB_INT", "OPS$ORACLE",
        "GGSYS", "FLOWS_FILES", "APEX_PUBLIC_USER", "GSMROOTUSER", "SYSRAC"
    );

    public static final JdbcAgentProfile OCEANBASE_ORACLE_PROFILE = new JdbcAgentProfile(
        "com.oceanbase.jdbc.Driver",
        "jdbc:oceanbase://{host}:{port}/{database}",
        2881,
        false,
        SYSTEM_SCHEMAS,
        List.of("TABLE", "VIEW", "BASE TABLE")
    ) {
        @Override
        public String schemaSwitchSql(String schema, String quote) {
            return "ALTER SESSION SET CURRENT_SCHEMA = " + quote + schema.replace(quote, quote + quote) + quote;
        }
    };

    public OceanBaseOracleAgent() {
        super(OCEANBASE_ORACLE_PROFILE);
    }

    @Override
    protected String buildJdbcUrl(ConnectParams params) {
        return buildUrl(params);
    }

    static String buildUrl(ConnectParams params) {
        return appendDefaultCompatibilityOption(OCEANBASE_ORACLE_PROFILE.buildUrl(params));
    }

    @Override
    public List<DatabaseInfo> listDatabases() {
        return unchecked(() -> {
            List<DatabaseInfo> result = new ArrayList<>();
            for (String schema : querySchemas()) {
                result.add(new DatabaseInfo(schema));
            }
            return result;
        });
    }

    @Override
    public List<String> listSchemas() {
        return unchecked(this::querySchemas);
    }

    @Override
    public List<TableInfo> listTables(String schema) {
        return unchecked(() -> {
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT o.OBJECT_NAME,
                    CASE o.OBJECT_TYPE WHEN 'VIEW' THEN 'VIEW' ELSE 'TABLE' END AS TABLE_TYPE,
                    c.COMMENTS
                FROM ALL_OBJECTS o
                LEFT JOIN ALL_TAB_COMMENTS c ON c.OWNER = o.OWNER AND c.TABLE_NAME = o.OBJECT_NAME
                WHERE o.OWNER = ? AND o.OBJECT_TYPE IN ('TABLE', 'VIEW')
                ORDER BY o.OBJECT_NAME
                """.stripIndent().trim();

            List<TableInfo> result = new ArrayList<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new TableInfo(rs.getString(1), rs.getString(2), rs.getString(3)));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<ObjectInfo> listObjects(String schema) {
        return unchecked(() -> {
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT OBJECT_NAME, OBJECT_TYPE
                FROM ALL_OBJECTS
                WHERE OWNER = ? AND OBJECT_TYPE IN ('TABLE', 'VIEW', 'PROCEDURE', 'FUNCTION', 'PACKAGE', 'SEQUENCE', 'SYNONYM')
                ORDER BY CASE OBJECT_TYPE
                    WHEN 'TABLE' THEN 0
                    WHEN 'VIEW' THEN 1
                    WHEN 'PROCEDURE' THEN 2
                    WHEN 'FUNCTION' THEN 3
                    WHEN 'PACKAGE' THEN 4
                    WHEN 'SEQUENCE' THEN 5
                    ELSE 6
                END, OBJECT_NAME
                """.stripIndent().trim();

            List<ObjectInfo> result = new ArrayList<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ObjectInfo(rs.getString(1), rs.getString(2), owner, null));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<ColumnInfo> getColumns(String schema, String table) {
        return unchecked(() -> {
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT c.COLUMN_NAME, c.DATA_TYPE, c.NULLABLE, c.DATA_PRECISION, c.DATA_SCALE,
                    c.DATA_LENGTH, c.CHAR_LENGTH, cc.COMMENTS,
                    CASE WHEN pk.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS IS_PK
                FROM ALL_TAB_COLUMNS c
                LEFT JOIN ALL_COL_COMMENTS cc
                    ON cc.OWNER = c.OWNER AND cc.TABLE_NAME = c.TABLE_NAME AND cc.COLUMN_NAME = c.COLUMN_NAME
                LEFT JOIN (
                    SELECT cols.COLUMN_NAME
                    FROM ALL_CONS_COLUMNS cols
                    JOIN ALL_CONSTRAINTS cons
                        ON cols.CONSTRAINT_NAME = cons.CONSTRAINT_NAME AND cols.OWNER = cons.OWNER
                    WHERE cons.CONSTRAINT_TYPE = 'P' AND cons.OWNER = ? AND cons.TABLE_NAME = ?
                ) pk ON pk.COLUMN_NAME = c.COLUMN_NAME
                WHERE c.OWNER = ? AND c.TABLE_NAME = ?
                ORDER BY c.COLUMN_ID
                """.stripIndent().trim();

            List<ColumnInfo> result = new ArrayList<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                stmt.setString(2, table);
                stmt.setString(3, owner);
                stmt.setString(4, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String name = rs.getString("COLUMN_NAME");
                        String baseType = rs.getString("DATA_TYPE");
                        Integer numPrec = intOrNull(rs, "DATA_PRECISION");
                        Integer numScale = intOrNull(rs, "DATA_SCALE");
                        Integer dataLen = intOrNull(rs, "DATA_LENGTH");
                        Integer charLen = intOrNull(rs, "CHAR_LENGTH");
                        result.add(new ColumnInfo(
                            name,
                            formatDataType(baseType, numPrec, numScale, dataLen, charLen),
                            "Y".equalsIgnoreCase(rs.getString("NULLABLE")),
                            null,
                            rs.getInt("IS_PK") == 1,
                            null,
                            rs.getString("COMMENTS"),
                            numPrec,
                            numScale,
                            charLen
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
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT i.INDEX_NAME, ic.COLUMN_NAME, ic.COLUMN_POSITION, i.UNIQUENESS,
                    c.CONSTRAINT_TYPE, i.INDEX_TYPE
                FROM ALL_INDEXES i
                JOIN ALL_IND_COLUMNS ic
                    ON i.INDEX_NAME = ic.INDEX_NAME
                    AND i.OWNER = ic.INDEX_OWNER
                    AND i.TABLE_OWNER = ic.TABLE_OWNER
                    AND i.TABLE_NAME = ic.TABLE_NAME
                LEFT JOIN ALL_CONSTRAINTS c
                    ON i.INDEX_NAME = c.INDEX_NAME
                    AND i.TABLE_OWNER = c.OWNER
                    AND i.TABLE_NAME = c.TABLE_NAME
                    AND c.CONSTRAINT_TYPE = 'P'
                WHERE i.TABLE_OWNER = ? AND i.TABLE_NAME = ?
                ORDER BY i.INDEX_NAME, ic.COLUMN_POSITION
                """.stripIndent().trim();

            Map<String, List<String>> columnsByIndex = new LinkedHashMap<>();
            Map<String, Boolean> uniqueByIndex = new LinkedHashMap<>();
            Map<String, Boolean> primaryByIndex = new LinkedHashMap<>();
            Map<String, String> typeByIndex = new LinkedHashMap<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String indexName = rs.getString("INDEX_NAME");
                        columnsByIndex.computeIfAbsent(indexName, ignored -> new ArrayList<>()).add(rs.getString("COLUMN_NAME"));
                        uniqueByIndex.put(indexName, "UNIQUE".equalsIgnoreCase(rs.getString("UNIQUENESS")));
                        primaryByIndex.put(indexName, "P".equalsIgnoreCase(rs.getString("CONSTRAINT_TYPE")));
                        typeByIndex.put(indexName, rs.getString("INDEX_TYPE"));
                    }
                }
            }

            List<IndexInfo> result = new ArrayList<>();
            for (Map.Entry<String, List<String>> entry : columnsByIndex.entrySet()) {
                String name = entry.getKey();
                result.add(new IndexInfo(
                    name,
                    entry.getValue(),
                    Boolean.TRUE.equals(uniqueByIndex.get(name)),
                    Boolean.TRUE.equals(primaryByIndex.get(name)),
                    null,
                    typeByIndex.get(name),
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
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT c.CONSTRAINT_NAME, cc.COLUMN_NAME, rc.TABLE_NAME, rcc.COLUMN_NAME
                FROM ALL_CONSTRAINTS c
                JOIN ALL_CONS_COLUMNS cc ON c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME AND c.OWNER = cc.OWNER
                JOIN ALL_CONSTRAINTS rc ON c.R_CONSTRAINT_NAME = rc.CONSTRAINT_NAME AND c.R_OWNER = rc.OWNER
                JOIN ALL_CONS_COLUMNS rcc
                    ON rc.CONSTRAINT_NAME = rcc.CONSTRAINT_NAME
                    AND rc.OWNER = rcc.OWNER
                    AND cc.POSITION = rcc.POSITION
                WHERE c.CONSTRAINT_TYPE = 'R' AND c.OWNER = ? AND c.TABLE_NAME = ?
                ORDER BY c.CONSTRAINT_NAME, cc.POSITION
                """.stripIndent().trim();

            List<ForeignKeyInfo> result = new ArrayList<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ForeignKeyInfo(
                            rs.getString(1),
                            rs.getString(2),
                            rs.getString(3),
                            rs.getString(4)
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
            String owner = normalizeSchema(schema);
            String sql = """
                SELECT TRIGGER_NAME, TRIGGERING_EVENT, TRIGGER_TYPE
                FROM ALL_TRIGGERS
                WHERE OWNER = ? AND TABLE_NAME = ?
                ORDER BY TRIGGER_NAME
                """.stripIndent().trim();

            List<TriggerInfo> result = new ArrayList<>();
            try (var stmt = requireConnection().prepareStatement(sql)) {
                stmt.setString(1, owner);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new TriggerInfo(rs.getString(1), rs.getString(2), rs.getString(3)));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public String setSchemaSQL(String schema) {
        return "ALTER SESSION SET CURRENT_SCHEMA = " + JdbcIdentifiers.INSTANCE.doubleQuote(schema);
    }

    private List<String> querySchemas() throws SQLException {
        try {
            return querySchemaNames();
        } catch (SQLException primaryError) {
            String current;
            try {
                current = currentSchema();
            } catch (SQLException ignored) {
                throw primaryError;
            }
            if (current == null || current.isBlank()) {
                throw primaryError;
            }
            return List.of(current);
        }
    }

    private List<String> querySchemaNames() throws SQLException {
        String placeholders = SYSTEM_SCHEMAS.stream()
            .map(schema -> "'" + schema + "'")
            .collect(Collectors.joining(","));
        String sql = """
            SELECT username
            FROM ALL_USERS
            WHERE username IS NOT NULL
              AND username NOT IN (%s)
              AND username NOT LIKE 'APEX_%%'
              AND username NOT LIKE 'FLOWS_%%'
              AND username NOT LIKE '%%$%%'
            ORDER BY CASE
                WHEN username = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') THEN 0
                WHEN username = SYS_CONTEXT('USERENV', 'SESSION_USER') THEN 1
                ELSE 2
            END, username
            """.formatted(placeholders).stripIndent().trim();

        List<String> result = new ArrayList<>();
        try (var stmt = requireConnection().createStatement();
             ResultSet rs = stmt.executeQuery(sql)) {
            while (rs.next()) {
                String schema = rs.getString(1);
                if (schema != null && !schema.isBlank()) {
                    result.add(schema);
                }
            }
        }
        return result;
    }

    private String normalizeSchema(String schema) throws SQLException {
        String value = schema == null ? "" : schema.trim();
        return value.isEmpty() ? currentSchema() : value;
    }

    private String currentSchema() throws SQLException {
        try (var stmt = requireConnection().createStatement();
             ResultSet rs = stmt.executeQuery("SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM DUAL")) {
            if (rs.next()) {
                String schema = rs.getString(1);
                if (schema != null && !schema.isBlank()) {
                    return schema;
                }
            }
        }
        return "";
    }

    private static String appendDefaultCompatibilityOption(String url) {
        if (hasQueryKey(url, COMPATIBLE_OJDBC_VERSION)) {
            return url;
        }
        return url + (url.contains("?") ? "&" : "?") + DEFAULT_COMPATIBLE_OJDBC_VERSION;
    }

    private static boolean hasQueryKey(String url, String key) {
        int queryStart = url.indexOf('?');
        if (queryStart < 0) {
            return false;
        }
        String query = url.substring(queryStart + 1);
        int fragmentStart = query.indexOf('#');
        if (fragmentStart >= 0) {
            query = query.substring(0, fragmentStart);
        }
        for (String part : query.split("[&;]")) {
            String normalized = part.trim();
            if (normalized.isEmpty()) {
                continue;
            }
            int equals = normalized.indexOf('=');
            String paramKey = equals >= 0 ? normalized.substring(0, equals) : normalized;
            if (paramKey.trim().equalsIgnoreCase(key)) {
                return true;
            }
        }
        return false;
    }

    private static String formatDataType(String base, Integer numPrec, Integer numScale, Integer dataLen, Integer charLen) {
        if (base == null || base.isBlank()) {
            return "";
        }
        return switch (base.toUpperCase(Locale.ROOT)) {
            case "VARCHAR2", "NVARCHAR2", "CHAR", "NCHAR" -> {
                Integer len = charLen == null ? dataLen : charLen;
                yield len == null ? base : base + "(" + len + ")";
            }
            case "NUMBER" -> {
                if (numPrec != null && numScale != null && numScale > 0) {
                    yield base + "(" + numPrec + "," + numScale + ")";
                }
                if (numPrec != null && numPrec > 0) {
                    yield base + "(" + numPrec + ")";
                }
                yield base;
            }
            case "RAW" -> dataLen == null ? "RAW" : "RAW(" + dataLen + ")";
            default -> base;
        };
    }

    private static Integer intOrNull(ResultSet rs, String column) throws SQLException {
        Object value = rs.getObject(column);
        return value instanceof Number ? ((Number) value).intValue() : null;
    }

    public static void main(String[] args) {
        new JsonRpcServer(new OceanBaseOracleAgent()).run();
    }
}
