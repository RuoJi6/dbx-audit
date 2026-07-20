import assert from "node:assert/strict";
import { afterEach, test, vi } from "vitest";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ConnectionConfig } from "../src/connections.js";

const pgMocks = vi.hoisted(() => ({
  poolConfigs: [] as Array<Record<string, unknown>>,
  query: vi.fn(),
  end: vi.fn().mockResolvedValue(undefined),
  on: vi.fn(),
}));

vi.mock("pg", () => ({
  default: {
    Pool: vi.fn(function MockPool(config: Record<string, unknown>) {
      pgMocks.poolConfigs.push(config);
      return {
        query: (sql: string, params?: unknown[]) => pgMocks.query(config, sql, params),
        end: pgMocks.end,
        on: pgMocks.on,
      };
    }),
  },
}));

import { closeDatabaseResources, executeQuery } from "../src/database.js";

function postgresConfig(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: `pg-ssl-${Math.random()}`,
    name: "pg-ssl",
    db_type: "postgres",
    host: "pg.example.com",
    port: 5432,
    username: "postgres",
    password: "secret",
    database: "app",
    ssl: false,
    ...overrides,
  };
}

function successfulQuery() {
  return { rows: [{ ok: true }], fields: [{ name: "ok" }] };
}

afterEach(async () => {
  await closeDatabaseResources();
  pgMocks.poolConfigs.length = 0;
  pgMocks.query.mockReset();
  pgMocks.end.mockClear();
  pgMocks.on.mockClear();
});

test("explicit PostgreSQL prefer mode tries TLS and falls back only when SSL is unsupported", async () => {
  pgMocks.query.mockImplementation((config: { ssl?: unknown }) => {
    if (config.ssl === false) return Promise.resolve(successfulQuery());
    return Promise.reject(new Error("The server does not support SSL connections"));
  });

  const result = await executeQuery(postgresConfig({ url_params: "sslmode=prefer" }), "select true as ok");

  assert.deepEqual(result.rows, [{ ok: true }]);
  assert.equal(pgMocks.poolConfigs.length, 2);
  assert.deepEqual(pgMocks.poolConfigs[0]?.ssl, { rejectUnauthorized: false });
  assert.equal(pgMocks.poolConfigs[1]?.ssl, false);
  assert.equal(pgMocks.query.mock.calls.length, 2);
});

test("explicit prefer does not downgrade on authentication or certificate errors", async () => {
  pgMocks.query.mockRejectedValue(new Error('no pg_hba.conf entry for host "127.0.0.1", no encryption'));

  await assert.rejects(() => executeQuery(postgresConfig({ url_params: "sslmode=prefer" }), "select 1"), /no pg_hba\.conf entry/);

  assert.equal(pgMocks.poolConfigs.length, 1);
  assert.deepEqual(pgMocks.poolConfigs[0]?.ssl, { rejectUnauthorized: false });
});

test("implicit PostgreSQL SSL mode remains disabled", async () => {
  pgMocks.query.mockResolvedValue(successfulQuery());

  await executeQuery(postgresConfig(), "select 1");

  assert.equal(pgMocks.poolConfigs.length, 1);
  assert.equal(pgMocks.poolConfigs[0]?.ssl, false);
});

test("explicit disable uses plaintext and never retries TLS negotiation failures", async () => {
  pgMocks.query.mockRejectedValue(new Error("The server does not support SSL connections"));

  await assert.rejects(() => executeQuery(postgresConfig({ url_params: "sslmode=disable" }), "select 1"), /does not support SSL/);

  assert.equal(pgMocks.poolConfigs.length, 1);
  assert.equal(pgMocks.poolConfigs[0]?.ssl, false);
  assert.doesNotMatch(String(pgMocks.poolConfigs[0]?.connectionString), /sslmode=/);
});

test("require and verification modes never downgrade", async () => {
  for (const mode of ["require", "verify-ca", "verify-full", "verify_identity"] as const) {
    pgMocks.query.mockRejectedValueOnce(new Error("The server does not support SSL connections"));

    await assert.rejects(() => executeQuery(postgresConfig({ url_params: `sslmode=${mode}` }), "select 1"), /does not support SSL/);
  }

  assert.equal(pgMocks.poolConfigs.length, 4);
  assert.deepEqual(pgMocks.poolConfigs[0]?.ssl, { rejectUnauthorized: false });
  assert.equal(typeof (pgMocks.poolConfigs[1]?.ssl as { checkServerIdentity?: unknown }).checkServerIdentity, "function");
  assert.deepEqual(pgMocks.poolConfigs[2]?.ssl, {});
  assert.deepEqual(pgMocks.poolConfigs[3]?.ssl, {});
});

test("PostgreSQL SSL files are loaded into the ssl object and removed from the connection string", async () => {
  const directory = await mkdtemp(join(tmpdir(), "dbx-pg-ssl-"));
  const caPath = join(directory, "ca.pem");
  const certPath = join(directory, "client.pem");
  const keyPath = join(directory, "client.key");
  await Promise.all([writeFile(caPath, "ca"), writeFile(certPath, "cert"), writeFile(keyPath, "key")]);
  pgMocks.query.mockResolvedValue(successfulQuery());

  try {
    await executeQuery(
      postgresConfig({
        url_params: `sslmode=verify-full&sslrootcert=${encodeURIComponent(caPath)}&sslcert=${encodeURIComponent(certPath)}&sslkey=${encodeURIComponent(keyPath)}&application_name=dbx`,
      }),
      "select 1",
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }

  const poolConfig = pgMocks.poolConfigs[0];
  assert.equal(poolConfig?.connectionString, "postgres://postgres:secret@pg.example.com:5432/app?application_name=dbx");
  assert.deepEqual(poolConfig?.ssl, {
    ca: Buffer.from("ca"),
    cert: Buffer.from("cert"),
    key: Buffer.from("key"),
  });
});
