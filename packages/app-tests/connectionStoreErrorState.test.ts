import assert from "node:assert/strict";
import { createPinia, setActivePinia } from "pinia";
import { test, vi } from "vitest";
import { useConnectionStore } from "../../apps/desktop/src/stores/connectionStore.ts";
import type { ConnectionConfig } from "../../apps/desktop/src/types/database.ts";

function installMemoryStorage() {
  const values = new Map<string, string>();
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
      clear: () => values.clear(),
    },
  });
  return () => {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  };
}

function conn(id: string): ConnectionConfig {
  return {
    id,
    name: id,
    db_type: "postgres",
    host: "localhost",
    port: 5432,
    username: "postgres",
    password: "",
  };
}

test("successful disconnect clears the connection error", async () => {
  const restoreStorage = installMemoryStorage();
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (input) => {
    if (String(input) === "/api/connection/disconnect") {
      return new Response("null", { status: 200, headers: { "Content-Type": "application/json" } });
    }
    return new Response("unexpected request", { status: 500 });
  }) as typeof fetch;

  try {
    setActivePinia(createPinia());
    const store = useConnectionStore();
    store.addEphemeralConnection(conn("conn-1"));
    store.recordConnectionError("conn-1", new Error("metadata failed"));

    await store.disconnect("conn-1");

    assert.equal(store.connectionErrors["conn-1"], undefined);
  } finally {
    globalThis.fetch = originalFetch;
    restoreStorage();
  }
});

test("failed disconnect keeps the existing connection error", async () => {
  const restoreStorage = installMemoryStorage();
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (input) => {
    if (String(input) === "/api/connection/disconnect") {
      return new Response("disconnect failed", { status: 500 });
    }
    return new Response("unexpected request", { status: 500 });
  }) as typeof fetch;

  try {
    setActivePinia(createPinia());
    const store = useConnectionStore();
    store.addEphemeralConnection(conn("conn-1"));
    store.recordConnectionError("conn-1", new Error("metadata failed"));

    await assert.rejects(() => store.disconnect("conn-1"), /disconnect failed/);

    assert.equal(store.connectionErrors["conn-1"], "metadata failed");
  } finally {
    globalThis.fetch = originalFetch;
    restoreStorage();
  }
});

test("query errors mentioning connection do not mark the connection disconnected", async () => {
  const restoreStorage = installMemoryStorage();
  try {
    setActivePinia(createPinia());
    const store = useConnectionStore();
    store.addEphemeralConnection(conn("conn-1"));
    store.activeConnectionId = "conn-1";

    store.recordConnectionLostError("conn-1", new Error('relation "connection" does not exist'));

    assert.equal(store.connectedIds.has("conn-1"), true);
    assert.equal(store.activeConnectionId, "conn-1");
    await new Promise((resolve) => setTimeout(resolve, 0));
  } finally {
    restoreStorage();
  }
});

test("known backend connection errors mark the connection disconnected", async () => {
  const restoreStorage = installMemoryStorage();
  const messages = [
    "java.sql.SQLRecoverableException: 关闭的连接",
    "java.sql.SQLRecoverableException: 连接已关闭",
    "server closed session with no notification",
    "server closed the connection unexpectedly",
    "Error occurred while creating a new object: error communicating with the server",
    "ORA-02396: exceeded maximum idle time, please connect again",
    "Agent stdin not available",
    "Failed to write to agent stdin",
  ];

  try {
    for (const [index, message] of messages.entries()) {
      setActivePinia(createPinia());
      const store = useConnectionStore();
      const connectionId = `conn-${index}`;
      store.addEphemeralConnection(conn(connectionId));
      store.activeConnectionId = connectionId;

      const marked = store.recordConnectionLostError(connectionId, new Error(message));

      assert.equal(marked, true, message);
      assert.equal(store.connectedIds.has(connectionId), false, message);
      assert.equal(store.activeConnectionId, null, message);
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  } finally {
    restoreStorage();
  }
});

test("explicit lost-connection marker clears state without relying on error text", async () => {
  const restoreStorage = installMemoryStorage();
  try {
    setActivePinia(createPinia());
    const store = useConnectionStore();
    store.addEphemeralConnection(conn("conn-1"));
    store.activeConnectionId = "conn-1";

    store.markConnectionLost("conn-1", new Error("连接可能已断开，请刷新数据重试"));

    assert.equal(store.connectedIds.has("conn-1"), false);
    assert.equal(store.activeConnectionId, null);
    assert.equal(store.connectionErrors["conn-1"], "连接可能已断开，请刷新数据重试");
    await new Promise((resolve) => setTimeout(resolve, 0));
  } finally {
    restoreStorage();
  }
});

test("late original connect errors replace the generic timeout detail", async () => {
  vi.useFakeTimers();
  const restoreStorage = installMemoryStorage();
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (input) => {
    if (String(input) === "/api/connection/connect") {
      return new Promise<Response>((_, reject) => {
        setTimeout(() => reject(new Error("MySQL connection failed: raw socket timeout")), 3500);
      });
    }
    return new Response("unexpected request", { status: 500 });
  }) as typeof fetch;

  try {
    setActivePinia(createPinia());
    const store = useConnectionStore();
    const config = { ...conn("conn-1"), connect_timeout_secs: 1 };
    const connectPromise = store.connect(config);
    const timeoutRejection = assert.rejects(() => connectPromise, /Connection attempt timed out after 3s/);

    await vi.advanceTimersByTimeAsync(3000);
    await timeoutRejection;
    assert.equal(store.connectionErrors["conn-1"], "Connection attempt timed out after 3s. Please check the network or VPN and try again.");

    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();

    assert.match(store.connectionErrors["conn-1"], /Original database error returned after the UI timeout/);
    assert.match(store.connectionErrors["conn-1"], /MySQL connection failed: raw socket timeout/);
  } finally {
    vi.useRealTimers();
    globalThis.fetch = originalFetch;
    restoreStorage();
  }
});
