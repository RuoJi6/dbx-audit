package com.dbx.agent.oracle10g;

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
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public final class Oracle10gAgent extends BaseDatabaseAgent {
    private static final List<String> SYSTEM_SCHEMAS = Arrays.asList(
        "SYS", "SYSTEM", "SYSMAN", "DBSNMP", "SYSBACKUP", "SYSDG", "SYSKM", "OUTLN",
        "AUDSYS", "LBACSYS", "DVF", "DVSYS", "APPQOSSYS", "CTXSYS", "MDSYS", "MDDATA",
        "ORDSYS", "ORDDATA", "ORDPLUGINS", "XDB", "ANONYMOUS", "DIP", "EXFSYS",
        "GSMADMIN_INTERNAL", "GSMCATUSER", "GSMUSER", "OJVMSYS", "OLAPSYS",
        "ORACLE_OCM", "SI_INFORMTN_SCHEMA", "WMSYS", "XS$NULL", "DBSFWUSER",
        "REMOTE_SCHEDULER_AGENT", "PDBADMIN", "DGPDB_INT", "OPS$ORACLE",
        "GGSYS", "FLOWS_FILES", "APEX_PUBLIC_USER"
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
        Matcher offsetFetch = OFFSET_FETCH_RE.matcher(sql);
        if (offsetFetch.matches()) {
            String innerSql = offsetFetch.group(1);
            long offset = Long.parseLong(offsetFetch.group(2));
            long limit = Long.parseLong(offsetFetch.group(4));
            long upper = offset + limit;
            return "SELECT * FROM (SELECT a.*, ROWNUM rn__ FROM (" + innerSql + ") a WHERE ROWNUM <= " + upper + ") WHERE rn__ > " + offset;
        }

        Matcher fetchOnly = FETCH_ONLY_RE.matcher(sql);
        if (fetchOnly.matches()) {
            String innerSql = fetchOnly.group(1);
            long limit = Long.parseLong(fetchOnly.group(3));
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
            connection = DriverManager.getConnection(buildUrl(params), connectionProperties(params));
        });
    }

    @Override
    public boolean testConnection(ConnectParams params) {
        return unchecked(() -> {
            Class.forName("oracle.jdbc.OracleDriver");
            try (Connection conn = DriverManager.getConnection(buildUrl(params), connectionProperties(params))) {
                return conn.isValid(5);
            }
        });
    }

    @Override
    public List<DatabaseInfo> listDatabases() {
        return unchecked(() -> {
            List<DatabaseInfo> result = new ArrayList<>();
            String sql = listDatabasesSql(0);
            try (Statement stmt = requireConnected().createStatement();
                 ResultSet rs = stmt.executeQuery(sql)) {
                while (rs.next()) {
                    result.add(new DatabaseInfo(rs.getString(1)));
                }
            }
            return result;
        });
    }

    static String listDatabasesSql(int visibleSchemaCount) {
        StringBuilder sql = new StringBuilder()
            .append("SELECT owner FROM (")
            .append(" SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AS owner FROM DUAL")
            .append(" UNION")
            .append(" SELECT DISTINCT owner FROM all_tables")
            .append(" UNION")
            .append(" SELECT DISTINCT owner FROM all_views")
            .append(" )")
            .append(" WHERE owner IS NOT NULL")
            .append(" AND owner NOT IN (").append(quotedSystemSchemas()).append(")")
            .append(" AND owner NOT LIKE 'APEX_%'")
            .append(" AND owner NOT LIKE 'FLOWS_%'");
        if (visibleSchemaCount > 0) {
            sql.append(" AND owner IN (");
            for (int i = 0; i < visibleSchemaCount; i++) {
                if (i > 0) sql.append(",");
                sql.append("?");
            }
            sql.append(")");
        }
        sql.append(" ORDER BY owner");
        return sql.toString();
    }

    @Override
    public List<String> listSchemas() {
        return unchecked(() -> {
            List<String> result = new ArrayList<>();
            for (DatabaseInfo database : listDatabases()) {
                result.add(database.getName());
            }
            return result;
        });
    }

    @Override
    public List<String> listSchemas(List<String> visibleSchemas) {
        if (visibleSchemas == null) {
            return listSchemas();
        }
        if (visibleSchemas.isEmpty()) {
            return Collections.emptyList();
        }
        return unchecked(() -> {
            List<String> result = new ArrayList<>();
            try (PreparedStatement stmt = requireConnected().prepareStatement(listDatabasesSql(visibleSchemas.size()))) {
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
            List<TableInfo> result = new ArrayList<>();
            try (PreparedStatement stmt = requireConnected().prepareStatement(listTablesSql())) {
                stmt.setString(1, schema);
                stmt.setString(2, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new TableInfo(rs.getString(1), rs.getString(2), rs.getString(3)));
                    }
                }
            }
            return result;
        });
    }

    static String listTablesSql() {
        return "SELECT OBJECT_NAME, TABLE_TYPE, COMMENTS"
            + " FROM ("
            + " SELECT t.TABLE_NAME AS OBJECT_NAME,"
            + " 'TABLE' AS TABLE_TYPE,"
            + " CAST(NULL AS VARCHAR2(4000)) AS COMMENTS"
            + " FROM ALL_TABLES t"
            + " WHERE t.OWNER = ?"
            + " AND t.NESTED = 'NO'"
            + " UNION ALL"
            + " SELECT o.OBJECT_NAME,"
            + " 'VIEW' AS TABLE_TYPE,"
            + " CAST(NULL AS VARCHAR2(4000)) AS COMMENTS"
            + " FROM ALL_OBJECTS o"
            + " WHERE o.OWNER = ?"
            + " AND o.OBJECT_TYPE = 'VIEW'"
            + " ) ORDER BY OBJECT_NAME";
    }

    @Override
    public List<ObjectInfo> listObjects(String schema) {
        return unchecked(() -> {
            List<ObjectInfo> result = new ArrayList<>();
            try (PreparedStatement stmt = requireConnected().prepareStatement(listObjectsSql())) {
                stmt.setString(1, schema);
                stmt.setString(2, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ObjectInfo(rs.getString(1), rs.getString(2), schema, null));
                    }
                }
            }
            return result;
        });
    }

    static String listObjectsSql() {
        return "SELECT OBJECT_NAME, OBJECT_TYPE"
            + " FROM ("
            + " SELECT t.TABLE_NAME AS OBJECT_NAME,"
            + " 'TABLE' AS OBJECT_TYPE"
            + " FROM ALL_TABLES t"
            + " WHERE t.OWNER = ?"
            + " AND t.NESTED = 'NO'"
            + " UNION ALL"
            + " SELECT o.OBJECT_NAME,"
            + " o.OBJECT_TYPE"
            + " FROM ALL_OBJECTS o"
            + " WHERE o.OWNER = ?"
            + " AND o.OBJECT_TYPE IN ('VIEW', 'PROCEDURE', 'FUNCTION')"
            + " ) ORDER BY CASE OBJECT_TYPE WHEN 'TABLE' THEN 0 WHEN 'VIEW' THEN 1 WHEN 'PROCEDURE' THEN 2 ELSE 3 END, OBJECT_NAME";
    }

    @Override
    public ObjectSource getObjectSource(String schema, String name, String objectType) {
        return unchecked(() -> {
            String upperType = objectType.toUpperCase(Locale.ROOT);
            String source;
            if ("VIEW".equals(upperType)) {
                // ALL_VIEWS is more reliable than DBMS_METADATA.GET_DDL,
                // which fails on XE editions where XSL stylesheets are missing.
                String sql = "SELECT TEXT FROM ALL_VIEWS WHERE OWNER = ? AND VIEW_NAME = ?";
                try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                    stmt.setString(1, schema);
                    stmt.setString(2, name);
                    try (ResultSet rs = stmt.executeQuery()) {
                        source = rs.next() ? coalesce(rs.getString(1)) : "";
                    }
                }
            } else {
                String dbmsType;
                switch (upperType) {
                    case "PROCEDURE":
                        dbmsType = "PROCEDURE";
                        break;
                    case "FUNCTION":
                        dbmsType = "FUNCTION";
                        break;
                    default:
                        throw new IllegalArgumentException("Unsupported object type: " + objectType);
                }
                String sql = "SELECT DBMS_METADATA.GET_DDL(?, ?, ?) FROM DUAL";
                try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                    stmt.setString(1, dbmsType);
                    stmt.setString(2, name);
                    stmt.setString(3, schema);
                    try (ResultSet rs = stmt.executeQuery()) {
                        source = rs.next() ? coalesce(rs.getString(1)) : "";
                    }
                }
            }
            return new ObjectSource(name, objectType, schema, source);
        });
    }

    @Override
    public String getTableDdl(String schema, String table) {
        return unchecked(() -> {
            String sql = "SELECT DBMS_METADATA.GET_DDL(?, ?, ?) FROM DUAL";
            try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, "TABLE");
                stmt.setString(2, table);
                stmt.setString(3, schema);
                try (ResultSet rs = stmt.executeQuery()) {
                    if (rs.next()) {
                        return coalesce(rs.getString(1));
                    }
                }
            }
            throw new IllegalArgumentException("Table not found: " + schema + "." + table);
        });
    }

    @Override
    public List<ColumnInfo> getColumns(String schema, String table) {
        return unchecked(() -> {
            List<ColumnInfo> result = new ArrayList<>();
            String sql = "SELECT c.COLUMN_NAME, c.DATA_TYPE, c.NULLABLE, c.DATA_PRECISION, c.DATA_SCALE,"
                + " c.DATA_LENGTH, c.CHAR_LENGTH, cc.COMMENTS,"
                + " CASE WHEN pk.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS IS_PK"
                + " FROM ALL_TAB_COLUMNS c"
                + " LEFT JOIN ALL_COL_COMMENTS cc"
                + " ON cc.OWNER = c.OWNER AND cc.TABLE_NAME = c.TABLE_NAME AND cc.COLUMN_NAME = c.COLUMN_NAME"
                + " LEFT JOIN ("
                + " SELECT cols.COLUMN_NAME FROM ALL_CONS_COLUMNS cols"
                + " JOIN ALL_CONSTRAINTS cons"
                + " ON cols.CONSTRAINT_NAME = cons.CONSTRAINT_NAME AND cols.OWNER = cons.OWNER"
                + " WHERE cons.CONSTRAINT_TYPE = 'P' AND cons.OWNER = ? AND cons.TABLE_NAME = ?"
                + " ) pk ON pk.COLUMN_NAME = c.COLUMN_NAME"
                + " WHERE c.OWNER = ? AND c.TABLE_NAME = ?"
                + " ORDER BY c.COLUMN_ID";
            try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                stmt.setString(3, schema);
                stmt.setString(4, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        String name = rs.getString("COLUMN_NAME");
                        String baseType = rs.getString("DATA_TYPE");
                        Integer numPrec = intObject(rs, "DATA_PRECISION");
                        Integer numScale = intObject(rs, "DATA_SCALE");
                        Integer dataLen = intObject(rs, "DATA_LENGTH");
                        Integer charLen = intObject(rs, "CHAR_LENGTH");
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
            List<IndexInfo> result = new ArrayList<>();
            String sql = "SELECT i.INDEX_NAME,"
                + " RTRIM(XMLAGG(XMLELEMENT(e, ic.COLUMN_NAME || ',') ORDER BY ic.COLUMN_POSITION).EXTRACT('//text()').GETSTRINGVAL(), ',') AS COLUMNS,"
                + " i.UNIQUENESS,"
                + " CASE WHEN c.CONSTRAINT_TYPE = 'P' THEN 1 ELSE 0 END AS IS_PK,"
                + " i.INDEX_TYPE"
                + " FROM ALL_INDEXES i"
                + " JOIN ALL_IND_COLUMNS ic ON i.INDEX_NAME = ic.INDEX_NAME AND i.OWNER = ic.INDEX_OWNER AND i.TABLE_OWNER = ic.TABLE_OWNER"
                + " LEFT JOIN ALL_CONSTRAINTS c ON i.INDEX_NAME = c.INDEX_NAME AND i.TABLE_OWNER = c.OWNER"
                + " AND c.CONSTRAINT_TYPE = 'P'"
                + " WHERE i.TABLE_OWNER = ? AND i.TABLE_NAME = ?"
                + " GROUP BY i.INDEX_NAME, i.UNIQUENESS, c.CONSTRAINT_TYPE, i.INDEX_TYPE"
                + " ORDER BY i.INDEX_NAME";
            try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new IndexInfo(
                            rs.getString(1),
                            splitNonEmpty(coalesce(rs.getString(2)), ","),
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
            List<ForeignKeyInfo> result = new ArrayList<>();
            String sql = "SELECT c.CONSTRAINT_NAME, cc.COLUMN_NAME, rc.TABLE_NAME, rcc.COLUMN_NAME"
                + " FROM ALL_CONSTRAINTS c"
                + " JOIN ALL_CONS_COLUMNS cc ON c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME AND c.OWNER = cc.OWNER"
                + " JOIN ALL_CONSTRAINTS rc ON c.R_CONSTRAINT_NAME = rc.CONSTRAINT_NAME AND c.R_OWNER = rc.OWNER"
                + " JOIN ALL_CONS_COLUMNS rcc ON rc.CONSTRAINT_NAME = rcc.CONSTRAINT_NAME AND rc.OWNER = rcc.OWNER"
                + " WHERE c.CONSTRAINT_TYPE = 'R' AND c.OWNER = ? AND c.TABLE_NAME = ?"
                + " ORDER BY c.CONSTRAINT_NAME";
            try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
                stmt.setString(1, schema);
                stmt.setString(2, table);
                try (ResultSet rs = stmt.executeQuery()) {
                    while (rs.next()) {
                        result.add(new ForeignKeyInfo(rs.getString(1), rs.getString(2), rs.getString(3), rs.getString(4)));
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
            String sql = "SELECT TRIGGER_NAME, TRIGGERING_EVENT, TRIGGER_TYPE"
                + " FROM ALL_TRIGGERS"
                + " WHERE OWNER = ? AND TABLE_NAME = ?"
                + " ORDER BY TRIGGER_NAME";
            try (PreparedStatement stmt = requireConnected().prepareStatement(sql)) {
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
            this::stringResultValue
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
            this::stringResultValue
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
            this::stringResultValue
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

    private Object stringResultValue(ResultSet rs, int index, int sqlType) {
        return unchecked(() -> {
            return JdbcExecutor.stringResultValue(rs, index, sqlType);
        });
    }

    static String buildUrl(ConnectParams params) {
        String connectionString = params.getConnection_string();
        if (connectionString != null && !connectionString.trim().isEmpty()) {
            return connectionString;
        }
        return "jdbc:oracle:thin:@" + params.getHost() + ":" + params.getPort() + "/" + serviceName(params);
    }

    private static Properties connectionProperties(ConnectParams params) {
        Properties props = new Properties();
        props.put("user", params.getUsername());
        props.put("password", params.getPassword());
        if (params.getDatabase().toUpperCase(Locale.ROOT).startsWith("SYSDBA:")) {
            props.put("internal_logon", "SYSDBA");
        }
        return props;
    }

    private static String serviceName(ConnectParams params) {
        String serviceName = params.getDatabase();
        if (serviceName.toUpperCase(Locale.ROOT).startsWith("SYSDBA:")) {
            return serviceName.substring(7);
        }
        return serviceName;
    }

    private static String quotedSystemSchemas() {
        List<String> quoted = new ArrayList<>();
        for (String schema : SYSTEM_SCHEMAS) {
            quoted.add("'" + schema + "'");
        }
        return String.join(",", quoted);
    }

    private static String trimTrailingSemicolons(String sql) {
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
        return rewriteFetchFirst(trimTrailingSemicolons(trimmed));
    }

    private static String formatDataType(String base, Integer numPrec, Integer numScale, Integer dataLen, Integer charLen) {
        switch (base.toUpperCase(Locale.ROOT)) {
            case "VARCHAR2":
            case "NVARCHAR2":
            case "CHAR":
            case "NCHAR":
                Integer length = charLen != null ? charLen : dataLen;
                return length != null ? base + "(" + length + ")" : base;
            case "NUMBER":
                if (numPrec != null && numScale != null && numScale > 0) {
                    return base + "(" + numPrec + "," + numScale + ")";
                }
                return numPrec != null && numPrec > 0 ? base + "(" + numPrec + ")" : base;
            case "RAW":
                return dataLen != null ? "RAW(" + dataLen + ")" : "RAW";
            default:
                return base;
        }
    }

    private static Integer intObject(ResultSet rs, String column) throws Exception {
        Object value = rs.getObject(column);
        return value == null ? null : ((Number) value).intValue();
    }

    private static List<String> splitNonEmpty(String value, String delimiter) {
        List<String> result = new ArrayList<>();
        for (String part : value.split(delimiter)) {
            if (!part.isEmpty()) {
                result.add(part);
            }
        }
        return result;
    }

    private static String coalesce(String value) {
        return value == null ? "" : value;
    }

    public static void main(String[] args) {
        new JsonRpcServer(new Oracle10gAgent()).run();
    }
}
