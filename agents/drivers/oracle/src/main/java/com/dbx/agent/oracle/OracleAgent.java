package com.dbx.agent.oracle;

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
import com.dbx.agent.QueryPageOptions;
import com.dbx.agent.QueryPageResult;
import com.dbx.agent.QueryResult;
import com.dbx.agent.TableInfo;
import com.dbx.agent.TriggerInfo;

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Collectors;

public class OracleAgent extends BaseDatabaseAgent {
    private static final Set<String> SYSTEM_SCHEMAS = Set.of(
        "SYS", "SYSTEM", "SYSMAN", "DBSNMP", "SYSBACKUP", "SYSDG", "SYSKM", "OUTLN",
        "AUDSYS", "LBACSYS", "DVF", "DVSYS", "APPQOSSYS", "CTXSYS", "MDSYS", "MDDATA",
        "ORDSYS", "ORDDATA", "ORDPLUGINS", "XDB", "ANONYMOUS", "DIP", "EXFSYS",
        "GSMADMIN_INTERNAL", "GSMCATUSER", "GSMUSER", "OJVMSYS", "OLAPSYS",
        "ORACLE_OCM", "SI_INFORMTN_SCHEMA", "WMSYS", "XS$NULL", "DBSFWUSER",
        "REMOTE_SCHEDULER_AGENT", "PDBADMIN", "DGPDB_INT", "OPS$ORACLE",
        "GGSYS", "FLOWS_FILES", "APEX_PUBLIC_USER", "GSMROOTUSER", "SYSRAC"
    );

    private static final Pattern OFFSET_FETCH_RE = Pattern.compile(
        "(.+?)\\s+OFFSET\\s+(\\d+)\\s+ROWS?\\s+FETCH\\s+(FIRST|NEXT)\\s+(\\d+)\\s+ROWS?\\s+ONLY",
        Pattern.CASE_INSENSITIVE
    );
    private static final Pattern FETCH_ONLY_RE = Pattern.compile(
        "(.+?)\\s+FETCH\\s+(FIRST|NEXT)\\s+(\\d+)\\s+ROWS?\\s+ONLY",
        Pattern.CASE_INSENSITIVE
    );
    private static final Pattern PLSQL_OBJECT_DDL_RE = Pattern.compile(
        "^\\s*CREATE\\s+(?:OR\\s+REPLACE\\s+)?(?:(?:NON)?EDITIONABLE\\s+)?(?:PROCEDURE|FUNCTION|PACKAGE(?:\\s+BODY)?|TRIGGER|TYPE(?:\\s+BODY)?)\\b",
        Pattern.CASE_INSENSITIVE
    );

    private Connection connection;

    @Override
    public Connection getConnection() {
        return connection;
    }

    public static String rewriteFetchFirst(String sql) {
        Matcher offsetMatcher = OFFSET_FETCH_RE.matcher(sql);
        if (offsetMatcher.matches()) {
            String innerSql = offsetMatcher.group(1);
            long offset = Long.parseLong(offsetMatcher.group(2));
            long limit = Long.parseLong(offsetMatcher.group(4));
            long upper = offset + limit;
            return "SELECT * FROM (SELECT a.*, ROWNUM rn__ FROM (" + innerSql + ") a WHERE ROWNUM <= " + upper
                + ") WHERE rn__ > " + offset;
        }

        Matcher fetchMatcher = FETCH_ONLY_RE.matcher(sql);
        if (fetchMatcher.matches()) {
            String innerSql = fetchMatcher.group(1);
            long limit = Long.parseLong(fetchMatcher.group(3));
            return "SELECT * FROM (" + innerSql + ") WHERE ROWNUM <= " + limit;
        }

        return sql;
    }

    @Override
    public String setSchemaSQL(String schema) {
        return "ALTER SESSION SET CURRENT_SCHEMA = " + JdbcIdentifiers.INSTANCE.doubleQuote(schema);
    }

    @Override
    public void connect(ConnectParams params) {
        uncheckedVoid(() -> {
            Class.forName("oracle.jdbc.OracleDriver");
            String serviceName = params.getDatabase();
            Properties props = connectionProperties(params);

            if (serviceName.toUpperCase(Locale.ROOT).startsWith("SYSDBA:")) {
                serviceName = serviceName.substring(7);
                props.setProperty("internal_logon", "SYSDBA");
            }

            connection = DriverManager.getConnection(buildUrl(params, serviceName), props);
            try (var stmt = connection.createStatement()) {
                stmt.execute("ALTER SESSION SET NLS_LANGUAGE='AMERICAN'");
            }
        });
    }

    @Override
    public boolean testConnection(ConnectParams params) {
        return unchecked(() -> {
            Class.forName("oracle.jdbc.OracleDriver");
            String serviceName = params.getDatabase();
            Properties props = connectionProperties(params);

            if (serviceName.toUpperCase(Locale.ROOT).startsWith("SYSDBA:")) {
                serviceName = serviceName.substring(7);
                props.setProperty("internal_logon", "SYSDBA");
            }

            try (Connection conn = DriverManager.getConnection(buildUrl(params, serviceName), props)) {
                return conn.isValid(5);
            }
        });
    }

    @Override
    public List<DatabaseInfo> listDatabases() {
        return unchecked(() -> {
            try {
                return queryDatabaseInfos(listDatabasesSql());
            } catch (SQLException e) {
                if (isPgaLimitError(e)) {
                    return currentSchemaDatabase();
                }
                throw e;
            }
        });
    }

    static String listDatabasesSql() {
        return listDatabasesSql(0);
    }

    static String listDatabasesSql(int visibleSchemaCount) {
        String placeholders = SYSTEM_SCHEMAS.stream()
            .map(schema -> "'" + schema + "'")
            .collect(Collectors.joining(","));
        String visibleSchemaClause = visibleSchemaCount <= 0 ? "" : "  AND username IN (" + "?,".repeat(visibleSchemaCount).replaceAll(",$", "") + ")\n";
        return """
            SELECT username AS owner
            FROM all_users
            WHERE username IS NOT NULL
              AND username NOT IN (%s)
              AND username NOT LIKE 'APEX_%%'
              AND username NOT LIKE 'FLOWS_%%'
              AND username NOT LIKE '%%$%%'
            %s
            ORDER BY CASE
                WHEN username = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') THEN 0
                WHEN username = SYS_CONTEXT('USERENV', 'SESSION_USER') THEN 1
                ELSE 2
            END, username
            """.formatted(placeholders, visibleSchemaClause).stripIndent().trim();
    }

    static boolean isPgaLimitError(SQLException error) {
        for (Throwable current = error; current != null; current = current.getCause()) {
            if (current instanceof SQLException sqlException) {
                if (sqlException.getErrorCode() == 4036 || String.valueOf(sqlException.getMessage()).contains("ORA-04036")) {
                    return true;
                }
                SQLException next = sqlException.getNextException();
                while (next != null) {
                    if (next.getErrorCode() == 4036 || String.valueOf(next.getMessage()).contains("ORA-04036")) {
                        return true;
                    }
                    next = next.getNextException();
                }
            } else if (String.valueOf(current.getMessage()).contains("ORA-04036")) {
                return true;
            }
        }
        return false;
    }

    private List<DatabaseInfo> queryDatabaseInfos(String sql) throws SQLException {
        List<DatabaseInfo> result = new ArrayList<>();
        try (var stmt = requireConnected().createStatement();
             ResultSet rs = stmt.executeQuery(sql)) {
            while (rs.next()) {
                result.add(new DatabaseInfo(rs.getString(1)));
            }
        }
        return result;
    }

    private List<DatabaseInfo> currentSchemaDatabase() throws SQLException {
        try (var stmt = requireConnected().createStatement();
             ResultSet rs = stmt.executeQuery("SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM DUAL")) {
            if (rs.next()) {
                String schema = rs.getString(1);
                if (schema != null && !schema.isBlank()) {
                    return List.of(new DatabaseInfo(schema));
                }
            }
        }
        return List.of();
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
    public List<String> listSchemas(List<String> visibleSchemas) {
        if (visibleSchemas == null) {
            return listSchemas();
        }
        if (visibleSchemas.isEmpty()) {
            return List.of();
        }
        return unchecked(() -> {
            List<String> result = new ArrayList<>();
            try (var stmt = requireConnected().prepareStatement(listDatabasesSql(visibleSchemas.size()))) {
                for (int i = 0; i < visibleSchemas.size(); i++) {
                    stmt.setString(i + 1, visibleSchemas.get(i));
                }
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(rs.getString(1));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<TableInfo> listTables(String schema) {
        return unchecked(() -> {
            try {
                List<TableInfo> result = new ArrayList<>();
                try (var stmt = requireConnected().prepareStatement(listTablesSql())) {
                    stmt.setString(1, schema);
                    stmt.setString(2, schema);
                    try (ResultSet rs = stmt.executeQuery()) {
                        while (rs.next()) {
                            result.add(new TableInfo(rs.getString(1), rs.getString(2), rs.getString(3)));
                        }
                    }
                }
                return result;
            } catch (SQLException e) {
                if (isPgaLimitError(e)) {
                    return List.of();
                }
                throw e;
            }
        });
    }

    static String listTablesSql() {
        return """
            SELECT OBJECT_NAME, TABLE_TYPE, COMMENTS
            FROM (
                SELECT t.TABLE_NAME AS OBJECT_NAME,
                    'TABLE' AS TABLE_TYPE,
                    CAST(NULL AS VARCHAR2(4000)) AS COMMENTS
                FROM ALL_TABLES t
                WHERE t.OWNER = ?
                  AND t.NESTED = 'NO'
                UNION ALL
                SELECT o.OBJECT_NAME,
                    'VIEW' AS TABLE_TYPE,
                    CAST(NULL AS VARCHAR2(4000)) AS COMMENTS
                FROM ALL_OBJECTS o
                WHERE o.OWNER = ?
                  AND o.OBJECT_TYPE = 'VIEW'
            )
            ORDER BY OBJECT_NAME
            """.stripIndent().trim();
    }

    @Override
    public List<ObjectInfo> listObjects(String schema) {
        return unchecked(() -> {
            try {
                List<ObjectInfo> result = new ArrayList<>();
                try (var stmt = requireConnected().prepareStatement(listObjectsSql())) {
                    stmt.setString(1, schema);
                    stmt.setString(2, schema);
                    try (ResultSet rs = stmt.executeQuery()) {
                        while (rs.next()) {
                            result.add(new ObjectInfo(rs.getString(1), rs.getString(2), schema, null));
                        }
                    }
                }
                return result;
            } catch (SQLException e) {
                if (isPgaLimitError(e)) {
                    return List.of();
                }
                throw e;
            }
        });
    }

    static String listObjectsSql() {
        return """
            SELECT OBJECT_NAME, OBJECT_TYPE
            FROM (
                SELECT t.TABLE_NAME AS OBJECT_NAME,
                    'TABLE' AS OBJECT_TYPE
                FROM ALL_TABLES t
                WHERE t.OWNER = ?
                  AND t.NESTED = 'NO'
                UNION ALL
                SELECT o.OBJECT_NAME,
                    o.OBJECT_TYPE
                FROM ALL_OBJECTS o
                WHERE o.OWNER = ?
                  AND o.OBJECT_TYPE IN ('VIEW', 'PROCEDURE', 'FUNCTION')
            )
            ORDER BY CASE OBJECT_TYPE WHEN 'TABLE' THEN 0 WHEN 'VIEW' THEN 1 WHEN 'PROCEDURE' THEN 2 ELSE 3 END, OBJECT_NAME
            """.stripIndent().trim();
    }

    @Override
    public ObjectSource getObjectSource(String schema, String name, String objectType) {
        return unchecked(() -> {
            String upperType = objectType.toUpperCase(Locale.ROOT);
            String source = "";
            if ("VIEW".equals(upperType)) {
                // ALL_VIEWS is more reliable than DBMS_METADATA.GET_DDL,
                // which fails on XE editions where XSL stylesheets are missing.
                try (var stmt = requireConnected().prepareStatement(
                    "SELECT TEXT FROM ALL_VIEWS WHERE OWNER = ? AND VIEW_NAME = ?")) {
                    stmt.setString(1, schema);
                    stmt.setString(2, name);
                    try (ResultSet rs = stmt.executeQuery()) {
                        if (rs.next()) {
                            String value = rs.getString(1);
                            source = value == null ? "" : value;
                        }
                    }
                }
            } else {
                String dbmsType = switch (upperType) {
                    case "PROCEDURE" -> "PROCEDURE";
                    case "FUNCTION" -> "FUNCTION";
                    default -> throw new IllegalArgumentException("Unsupported object type: " + objectType);
                };
                try (var stmt = requireConnected().prepareStatement(
                    "SELECT DBMS_METADATA.GET_DDL(?, ?, ?) FROM DUAL")) {
                    stmt.setString(1, dbmsType);
                    stmt.setString(2, name);
                    stmt.setString(3, schema);
                    try (ResultSet rs = stmt.executeQuery()) {
                        if (rs.next()) {
                            String value = rs.getString(1);
                            source = value == null ? "" : value;
                        }
                    }
                }
            }
            return new ObjectSource(name, objectType, schema, source);
        });
    }

    @Override
    public String getTableDdl(String schema, String table) {
        return unchecked(() -> {
            try (var stmt = requireConnected().prepareStatement("SELECT DBMS_METADATA.GET_DDL(?, ?, ?) FROM DUAL")) {
                stmt.setString(1, "TABLE");
                stmt.setString(2, table);
                stmt.setString(3, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    if (rs.next()) {
                        String value = rs.getString(1);
                        return value == null ? "" : value;
                    }
                }
            }
            throw new IllegalArgumentException("Table not found: " + schema + "." + table);
        });
    }

    @Override
    public List<ColumnInfo> getColumns(String schema, String table) {
        return unchecked(() -> {
            String sql = """
                SELECT c.COLUMN_NAME, c.DATA_TYPE, c.NULLABLE, c.DATA_PRECISION, c.DATA_SCALE,
                    c.DATA_LENGTH, c.CHAR_LENGTH, cc.COMMENTS,
                    CASE WHEN pk.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS IS_PK
                FROM ALL_TAB_COLUMNS c
                LEFT JOIN ALL_COL_COMMENTS cc
                    ON cc.OWNER = c.OWNER AND cc.TABLE_NAME = c.TABLE_NAME AND cc.COLUMN_NAME = c.COLUMN_NAME
                LEFT JOIN (
                    SELECT cols.COLUMN_NAME FROM ALL_CONS_COLUMNS cols
                    JOIN ALL_CONSTRAINTS cons
                        ON cols.CONSTRAINT_NAME = cons.CONSTRAINT_NAME AND cols.OWNER = cons.OWNER
                    WHERE cons.CONSTRAINT_TYPE = 'P' AND cons.OWNER = ? AND cons.TABLE_NAME = ?
                ) pk ON pk.COLUMN_NAME = c.COLUMN_NAME
                WHERE c.OWNER = ? AND c.TABLE_NAME = ?
                ORDER BY c.COLUMN_ID
                """.stripIndent().trim();

            List<ColumnInfo> result = new ArrayList<>();
            try (var stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                stmt.setString(3, schema);
                stmt.setString(4, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String name = rs.getString("COLUMN_NAME");
                        String baseType = rs.getString("DATA_TYPE");
                        Integer numPrec = intOrNull(rs, "DATA_PRECISION");
                        Integer numScale = intOrNull(rs, "DATA_SCALE");
                        Integer dataLen = intOrNull(rs, "DATA_LENGTH");
                        Integer charLen = intOrNull(rs, "CHAR_LENGTH");
                        String dataType = formatDataType(baseType, numPrec, numScale, dataLen, charLen);

                        result.add(new ColumnInfo(
                            name,
                            dataType,
                            "Y".equals(rs.getString("NULLABLE")),
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
            String sql = """
                SELECT i.INDEX_NAME,
                    LISTAGG(ic.COLUMN_NAME, ',') WITHIN GROUP (ORDER BY ic.COLUMN_POSITION) AS COLUMNS,
                    i.UNIQUENESS,
                    CASE WHEN c.CONSTRAINT_TYPE = 'P' THEN 1 ELSE 0 END AS IS_PK,
                    i.INDEX_TYPE
                FROM ALL_INDEXES i
                JOIN ALL_IND_COLUMNS ic ON i.INDEX_NAME = ic.INDEX_NAME AND i.OWNER = ic.INDEX_OWNER AND i.TABLE_OWNER = ic.TABLE_OWNER
                LEFT JOIN ALL_CONSTRAINTS c ON i.INDEX_NAME = c.INDEX_NAME AND i.TABLE_OWNER = c.OWNER
                    AND c.CONSTRAINT_TYPE = 'P'
                WHERE i.TABLE_OWNER = ? AND i.TABLE_NAME = ?
                GROUP BY i.INDEX_NAME, i.UNIQUENESS, c.CONSTRAINT_TYPE, i.INDEX_TYPE
                ORDER BY i.INDEX_NAME
                """.stripIndent().trim();

            List<IndexInfo> result = new ArrayList<>();
            try (var stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String colsStr = rs.getString(2);
                        List<String> columns = new ArrayList<>();
                        if (colsStr != null && !colsStr.isEmpty()) {
                            columns.addAll(List.of(colsStr.split(",")));
                        }
                        result.add(new IndexInfo(
                            rs.getString(1),
                            columns,
                            "UNIQUE".equals(rs.getString(3)),
                            "1".equals(rs.getString(4)),
                            null,
                            rs.getString(5),
                            null,
                            null
                        ));
                    }
                }
            }
            return result;
        });
    }

    @Override
    public List<ForeignKeyInfo> listForeignKeys(String schema, String table) {
        return unchecked(() -> {
            String sql = """
                SELECT c.CONSTRAINT_NAME, cc.COLUMN_NAME, rc.TABLE_NAME, rcc.COLUMN_NAME
                FROM ALL_CONSTRAINTS c
                JOIN ALL_CONS_COLUMNS cc ON c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME AND c.OWNER = cc.OWNER
                JOIN ALL_CONSTRAINTS rc ON c.R_CONSTRAINT_NAME = rc.CONSTRAINT_NAME AND c.R_OWNER = rc.OWNER
                JOIN ALL_CONS_COLUMNS rcc ON rc.CONSTRAINT_NAME = rcc.CONSTRAINT_NAME AND rc.OWNER = rcc.OWNER
                WHERE c.CONSTRAINT_TYPE = 'R' AND c.OWNER = ? AND c.TABLE_NAME = ?
                ORDER BY c.CONSTRAINT_NAME
                """.stripIndent().trim();

            List<ForeignKeyInfo> result = new ArrayList<>();
            try (var stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
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
            String sql = """
                SELECT TRIGGER_NAME, TRIGGERING_EVENT, TRIGGER_TYPE
                FROM ALL_TRIGGERS
                WHERE OWNER = ? AND TABLE_NAME = ?
                ORDER BY TRIGGER_NAME
                """.stripIndent().trim();

            List<TriggerInfo> result = new ArrayList<>();
            try (var stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
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
    public QueryResult executeQuery(String sql, String schema, ExecuteQueryOptions options) {
        return JdbcExecutor.INSTANCE.execute(
            requireConnected(),
            prepareExecutableSql(sql),
            schema,
            this::setSchemaSQL,
            options.getMaxRows(),
            options.getFetchSize(),
            options.getTimeoutSecs(),
            OracleAgent::stringResultValue
        );
    }

    @Override
    public QueryPageResult executeQueryPage(String sql, String schema, QueryPageOptions options) {
        return JdbcExecutor.INSTANCE.executePage(
            requireConnected(),
            prepareExecutableSql(sql),
            schema,
            this::setSchemaSQL,
            options,
            OracleAgent::stringResultValue
        );
    }

    @Override
    public QueryPageResult startTableRead(String sql, String schema, QueryPageOptions options) {
        return JdbcExecutor.INSTANCE.startTableRead(
            requireConnected(),
            prepareExecutableSql(sql),
            schema,
            this::setSchemaSQL,
            options,
            OracleAgent::stringResultValue
        );
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

    private static Properties connectionProperties(ConnectParams params) {
        Properties props = new Properties();
        props.setProperty("user", params.getUsername());
        props.setProperty("password", params.getPassword());
        props.setProperty("oracle.jdbc.defaultNChar", "true");
        return props;
    }

    static String buildUrl(ConnectParams params) {
        return buildUrl(params, serviceName(params));
    }

    private static String buildUrl(ConnectParams params, String serviceName) {
        String connectionString = params.getConnection_string();
        if (connectionString != null && !connectionString.trim().isEmpty()) {
            return connectionString;
        }
        return "jdbc:oracle:thin:@" + params.getHost() + ":" + params.getPort() + "/" + serviceName;
    }

    private static String serviceName(ConnectParams params) {
        String serviceName = params.getDatabase();
        if (serviceName.toUpperCase(Locale.ROOT).startsWith("SYSDBA:")) {
            return serviceName.substring(7);
        }
        return serviceName;
    }

    private static String formatDataType(
        String base,
        Integer numPrec,
        Integer numScale,
        Integer dataLen,
        Integer charLen
    ) {
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

    private static Object stringResultValue(ResultSet rs, int index, int sqlType) {
        try {
            return JdbcExecutor.stringResultValue(rs, index, sqlType);
        } catch (SQLException e) {
            throw new RuntimeException(e);
        }
    }

    private static Integer intOrNull(ResultSet rs, String column) throws Exception {
        Object value = rs.getObject(column);
        return value instanceof Number ? ((Number) value).intValue() : null;
    }

    private static String trimEndSemicolons(String sql) {
        String trimmed = sql;
        while (trimmed.endsWith(";")) {
            trimmed = trimmed.substring(0, trimmed.length() - 1);
        }
        return trimmed;
    }

    static String prepareExecutableSql(String sql) {
        String trimmed = sql.trim();
        if (PLSQL_OBJECT_DDL_RE.matcher(trimmed).find()) {
            return trimmed;
        }
        return rewriteFetchFirst(trimEndSemicolons(trimmed));
    }

    public static void main(String[] args) {
        new JsonRpcServer(new OracleAgent()).run();
    }
}
