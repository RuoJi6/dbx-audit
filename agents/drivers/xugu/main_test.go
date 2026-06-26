package main

import (
	"encoding/json"
	"errors"
	"os"
	"strings"
	"testing"
)

func TestHandshakeResponse(t *testing.T) {
	s := newServer()
	resp, shutdown := s.handleLine(`{"jsonrpc":"2.0","id":7,"method":"handshake","params":{"appVersion":"dev"}}`)
	if shutdown {
		t.Fatal("handshake should not shut down the server")
	}
	if resp.Error != nil {
		t.Fatalf("unexpected error: %v", resp.Error)
	}
	data, err := json.Marshal(resp.Result)
	if err != nil {
		t.Fatal(err)
	}
	var result struct {
		ProtocolVersion      int      `json:"protocolVersion"`
		AgentProtocolVersion int      `json:"agentProtocolVersion"`
		Capabilities         []string `json:"capabilities"`
	}
	if err := json.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	if result.ProtocolVersion != 1 || result.AgentProtocolVersion != 1 {
		t.Fatalf("unexpected protocol versions: %+v", result)
	}
	contract := protocolContract(t)
	if result.ProtocolVersion != contract.ProtocolVersion || result.AgentProtocolVersion != contract.ProtocolVersion {
		t.Fatalf("handshake protocol versions do not match contract: result=%+v contract=%+v", result, contract)
	}
	for _, capability := range result.Capabilities {
		if !contains(contract.AllCapabilities, capability) {
			t.Fatalf("handshake returned capability %q outside protocol contract %v", capability, contract.AllCapabilities)
		}
	}
	if !contains(result.Capabilities, "query") || !contains(result.Capabilities, "metadata") {
		t.Fatalf("expected query and metadata capabilities, got %v", result.Capabilities)
	}
}

func TestCloseMissingQuerySessionReturnsFalse(t *testing.T) {
	s := newServer()
	resp, shutdown := s.handleLine(`{"jsonrpc":"2.0","id":8,"method":"close_query_session","params":{"sessionId":"missing"}}`)
	if shutdown {
		t.Fatal("close_query_session should not shut down the server")
	}
	if resp.Error != nil {
		t.Fatalf("unexpected error: %v", resp.Error)
	}
	if resp.Result != false {
		t.Fatalf("expected false result, got %#v", resp.Result)
	}
}

func TestEmptyResultSlicesMarshalAsArrays(t *testing.T) {
	data, err := json.Marshal(queryResult{})
	if err != nil {
		t.Fatal(err)
	}
	text := string(data)
	if strings.Contains(text, `"columns":null`) || strings.Contains(text, `"column_types":null`) || strings.Contains(text, `"rows":null`) {
		t.Fatalf("query result should marshal nil slices as arrays: %s", text)
	}

	data, err = json.Marshal(indexInfo{})
	if err != nil {
		t.Fatal(err)
	}
	text = string(data)
	if strings.Contains(text, `"columns":null`) || strings.Contains(text, `"included_columns":null`) {
		t.Fatalf("index info should marshal nil slices as arrays: %s", text)
	}
}

func TestGetTableDDLResultMarshalsAsString(t *testing.T) {
	data, err := json.Marshal("CREATE TABLE SYSDBA.ORDERS (ID INT)")
	if err != nil {
		t.Fatal(err)
	}
	var ddl string
	if err := json.Unmarshal(data, &ddl); err != nil {
		t.Fatalf("get_table_ddl result must deserialize as a string: %v", err)
	}
}

func TestBuildDSNUsesConnectionStringWhenProvided(t *testing.T) {
	dsn := buildDSN(connectParams{ConnectionString: "IP=db.example.com;DB=SYSTEM;User=SYSDBA;PWD=secret;Port=5138"})

	if dsn != "IP=db.example.com;DB=SYSTEM;User=SYSDBA;PWD=secret;Port=5138" {
		t.Fatalf("unexpected dsn: %s", dsn)
	}
}

func protocolContract(t *testing.T) struct {
	ProtocolVersion int      `json:"protocolVersion"`
	AllCapabilities []string `json:"allCapabilities"`
} {
	t.Helper()
	data, err := os.ReadFile("../../common/src/main/resources/agent-protocol-v1.json")
	if err != nil {
		t.Fatal(err)
	}
	var contract struct {
		ProtocolVersion int      `json:"protocolVersion"`
		AllCapabilities []string `json:"allCapabilities"`
	}
	if err := json.Unmarshal(data, &contract); err != nil {
		t.Fatal(err)
	}
	return contract
}

func TestBuildDSNUsesConnectionFields(t *testing.T) {
	dsn := buildDSN(connectParams{
		Host:     "db.example.com",
		Port:     15138,
		Database: "demo",
		Username: "sysdba",
		Password: "secret",
	})

	for _, part := range []string{"IP=db.example.com", "DB=demo", "User=sysdba", "PWD=secret", "Port=15138"} {
		if !strings.Contains(dsn, part) {
			t.Fatalf("dsn should contain %s, got: %s", part, dsn)
		}
	}
}

func TestBuildDSNUsesDefaultPort(t *testing.T) {
	dsn := buildDSN(connectParams{
		Host:     "db.example.com",
		Database: "demo",
		Username: "sysdba",
		Password: "secret",
	})

	if !strings.Contains(dsn, "Port=5138") {
		t.Fatalf("dsn should default to Xugu port, got: %s", dsn)
	}
}

func TestBuildDSNParsesJdbcURL(t *testing.T) {
	dsn := buildDSN(connectParams{
		Username:         "sysdba",
		Password:         "secret",
		ConnectionString: "jdbc:xugu://db.example.com:15138/demo",
	})

	for _, part := range []string{"IP=db.example.com", "DB=demo", "User=sysdba", "PWD=secret", "Port=15138"} {
		if !strings.Contains(dsn, part) {
			t.Fatalf("dsn should contain %s, got: %s", part, dsn)
		}
	}
}

func TestBuildDSNParsesDBXURL(t *testing.T) {
	dsn := buildDSN(connectParams{
		ConnectionString: "xugu://sysdba:secret@db.example.com:15138/demo",
	})

	for _, part := range []string{"IP=db.example.com", "DB=demo", "User=sysdba", "PWD=secret", "Port=15138"} {
		if !strings.Contains(dsn, part) {
			t.Fatalf("dsn should contain %s, got: %s", part, dsn)
		}
	}
}

func TestBuildDSNAppendsURLParams(t *testing.T) {
	dsn := buildDSN(connectParams{
		Host:      "db.example.com",
		Database:  "demo",
		Username:  "sysdba",
		Password:  "secret",
		URLParams: "AUTO_COMMIT=on;CHAR_SET=UTF8",
	})

	for _, part := range []string{"AUTO_COMMIT=on", "CHAR_SET=UTF8"} {
		if !strings.Contains(dsn, part) {
			t.Fatalf("dsn should contain %s, got: %s", part, dsn)
		}
	}
}

func TestBuildDSNDefaultsToUTF8(t *testing.T) {
	dsn := buildDSN(connectParams{
		Host:     "db.example.com",
		Database: "demo",
		Username: "sysdba",
		Password: "secret",
	})

	if !strings.Contains(dsn, "CHAR_SET=UTF8") {
		t.Fatalf("dsn should default to UTF8, got: %s", dsn)
	}
}

func TestBuildDSNRespectsExplicitCharset(t *testing.T) {
	dsn := buildDSN(connectParams{
		Host:      "db.example.com",
		Database:  "demo",
		Username:  "sysdba",
		Password:  "secret",
		URLParams: "CHAR_SET=GBK",
	})

	if strings.Contains(dsn, "CHAR_SET=UTF8") || !strings.Contains(dsn, "CHAR_SET=GBK") {
		t.Fatalf("dsn should respect explicit charset, got: %s", dsn)
	}
}

func TestListDatabasesSQLUsesXuguDictionary(t *testing.T) {
	sqlText := strings.ToUpper(xuguListDatabasesSQL)

	if !strings.Contains(sqlText, "ALL_DATABASES") || strings.Contains(sqlText, "SYS_DATABASES") {
		t.Fatalf("database listing should query low-privilege ALL_DATABASES, got: %s", xuguListDatabasesSQL)
	}
}

func TestFallbackDatabasesFromParams(t *testing.T) {
	cases := []struct {
		name   string
		params connectParams
		want   string
	}{
		{
			name: "database field",
			params: connectParams{
				Database: "LOWPRIV",
			},
			want: "LOWPRIV",
		},
		{
			name: "dbx url",
			params: connectParams{
				ConnectionString: "xugu://user:secret@db.example.com:5138/demo",
			},
			want: "demo",
		},
		{
			name: "jdbc url",
			params: connectParams{
				ConnectionString: "jdbc:xugu://db.example.com:5138/reporting",
			},
			want: "reporting",
		},
		{
			name: "native dsn",
			params: connectParams{
				ConnectionString: "IP=db.example.com;DB=SYSTEM;User=SYSDBA;PWD=secret;Port=5138",
			},
			want: "SYSTEM",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := fallbackDatabasesFromParams(tc.params)
			if len(got) != 1 || got[0].Name != tc.want {
				t.Fatalf("unexpected fallback databases: got=%v want=%s", got, tc.want)
			}
		})
	}
}

func TestUseDatabaseSkipsConfiguredDatabase(t *testing.T) {
	s := newServer()
	s.params = connectParams{Database: "SYSTEM"}

	if err := s.useDatabase("system"); err != nil {
		t.Fatalf("expected configured database USE to be skipped, got: %v", err)
	}
}

func TestConfiguredDatabaseName(t *testing.T) {
	cases := []struct {
		params connectParams
		want   string
	}{
		{params: connectParams{Database: "SYSTEM"}, want: "SYSTEM"},
		{params: connectParams{ConnectionString: "xugu://user:secret@db.example.com:5138/demo"}, want: "demo"},
		{params: connectParams{ConnectionString: "jdbc:xugu://db.example.com:5138/reporting"}, want: "reporting"},
		{params: connectParams{ConnectionString: "IP=db.example.com;DB=SYSTEM;User=SYSDBA;PWD=secret"}, want: "SYSTEM"},
	}

	for _, tc := range cases {
		if got := configuredDatabaseName(tc.params); got != tc.want {
			t.Fatalf("configuredDatabaseName(%+v) = %q, want %q", tc.params, got, tc.want)
		}
	}
}

func TestSchemaListingSQLUsesLowPrivilegeDictionary(t *testing.T) {
	sqlText := strings.ToUpper(xuguListSchemasSQL)

	if !strings.Contains(sqlText, "ALL_SCHEMAS") || strings.Contains(sqlText, "SYS_SCHEMAS") {
		t.Fatalf("schema listing should query low-privilege ALL_SCHEMAS, got: %s", xuguListSchemasSQL)
	}
}

func TestPrimaryKeySQLUsesLowPrivilegeDictionary(t *testing.T) {
	sqlText := strings.ToUpper(xuguPrimaryKeyColumnsSQL)

	for _, want := range []string{"ALL_CONSTRAINTS", "ALL_TABLES", "ALL_SCHEMAS"} {
		if !strings.Contains(sqlText, want) {
			t.Fatalf("primary key listing should query %s, got: %s", want, xuguPrimaryKeyColumnsSQL)
		}
	}
	for _, forbidden := range []string{"SYS_CONSTRAINTS", "SYS_TABLES", "SYS_SCHEMAS"} {
		if strings.Contains(sqlText, forbidden) {
			t.Fatalf("primary key listing should not query %s, got: %s", forbidden, xuguPrimaryKeyColumnsSQL)
		}
	}
}

func TestColumnSQLUsesLowPrivilegeDictionary(t *testing.T) {
	sqlText := strings.ToUpper(xuguListColumnsSQL)

	for _, want := range []string{"ALL_COLUMNS", "ALL_TABLES", "ALL_SCHEMAS", "COMMENTS"} {
		if !strings.Contains(sqlText, want) {
			t.Fatalf("column listing should query %s, got: %s", want, xuguListColumnsSQL)
		}
	}
	for _, forbidden := range []string{"SYS_COLUMNS", "SYS_TABLES", "SYS_SCHEMAS"} {
		if strings.Contains(sqlText, forbidden) {
			t.Fatalf("column listing should not query %s, got: %s", forbidden, xuguListColumnsSQL)
		}
	}
}

func TestIndexSQLUsesLowPrivilegeDictionary(t *testing.T) {
	sqlText := strings.ToUpper(xuguListIndexesSQL)

	for _, want := range []string{"ALL_INDEXES", "ALL_TABLES", "ALL_SCHEMAS", "KEYS"} {
		if !strings.Contains(sqlText, want) {
			t.Fatalf("index listing should query %s, got: %s", want, xuguListIndexesSQL)
		}
	}
	for _, forbidden := range []string{"SYS_INDEXES", "SYS_TABLES", "SYS_SCHEMAS"} {
		if strings.Contains(sqlText, forbidden) {
			t.Fatalf("index listing should not query %s, got: %s", forbidden, xuguListIndexesSQL)
		}
	}
}

func TestXuguMetadataAccessErrorDetection(t *testing.T) {
	if !isXuguMetadataAccessError(errors.New("[E18012] 权限不够")) {
		t.Fatal("expected E18012 permission error to be treated as metadata access error")
	}
	if isXuguMetadataAccessError(errors.New("network timeout")) {
		t.Fatal("network errors should not trigger database-list fallback")
	}
}

func TestParseForeignKeyColumns(t *testing.T) {
	local, ref := parseForeignKeyColumns(`("C1","C2")("ID1","ID2")`)

	if strings.Join(local, ",") != "C1,C2" || strings.Join(ref, ",") != "ID1,ID2" {
		t.Fatalf("unexpected foreign key columns: local=%v ref=%v", local, ref)
	}
}

func TestDecodeXuguScale(t *testing.T) {
	numericScale := 32*65536 + 6
	precision, scale, length := decodeXuguScale("NUMERIC", &numericScale)
	if precision == nil || *precision != 32 || scale == nil || *scale != 6 || length != nil {
		t.Fatalf("unexpected numeric scale decode: precision=%v scale=%v length=%v", precision, scale, length)
	}

	charScale := 128
	precision, scale, length = decodeXuguScale("VARCHAR", &charScale)
	if precision != nil || scale != nil || length == nil || *length != 128 {
		t.Fatalf("unexpected char scale decode: precision=%v scale=%v length=%v", precision, scale, length)
	}
}

func TestAppendDDLStatement(t *testing.T) {
	got := appendDDLStatement("CREATE TABLE \"T\" (\"ID\" INT)\n", "CREATE INDEX \"IDX\" ON \"T\"(\"ID\");")
	want := "CREATE TABLE \"T\" (\"ID\" INT);\n\nCREATE INDEX \"IDX\" ON \"T\"(\"ID\");"

	if got != want {
		t.Fatalf("unexpected DDL append:\ngot:  %q\nwant: %q", got, want)
	}
}

func TestQuoteStringLiteralEscapesSingleQuotes(t *testing.T) {
	if got := quoteStringLiteral("owner's note"); got != "'owner''s note'" {
		t.Fatalf("unexpected quoted string: %s", got)
	}
}

func TestNormalizeValuePreservesDriverNumericTypes(t *testing.T) {
	if value := normalizeValue(int32(7)); value != int64(7) {
		t.Fatalf("expected int32 to normalize to int64, got %#v", value)
	}
	if value := normalizeValue(float32(1.25)); value != float64(float32(1.25)) {
		t.Fatalf("expected float32 to normalize to float64, got %#v", value)
	}
}

func contains(values []string, target string) bool {
	for _, value := range values {
		if value == target {
			return true
		}
	}
	return false
}
