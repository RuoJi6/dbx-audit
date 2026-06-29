import type { SqlSafetyOptions } from "./sql-safety.js";

export type RedisCommandSafety = "allowed" | "confirm" | "blocked";

export interface RedisCommandResult {
  command: string;
  safety: RedisCommandSafety;
  value: unknown;
}

export interface RedisCommandOptions {
  skipSafetyCheck?: boolean;
  timeoutMs?: number;
}

export interface RedisCommandSafetyDecision {
  allowed: boolean;
  command?: string;
  safety?: RedisCommandSafety;
  reason?: string;
  skipSafetyCheck?: boolean;
}

const BLOCKED_REDIS_COMMANDS = new Set([
  "KEYS",
  "FLUSHALL",
  "SHUTDOWN",
  "CONFIG",
  "SAVE",
  "BGSAVE",
  "SLAVEOF",
  "REPLICAOF",
  "MIGRATE",
  "MODULE",
  "SCRIPT",
  "EVAL",
  "EVALSHA",
]);

const CONFIRM_REDIS_COMMANDS = new Set([
  "DEL",
  "UNLINK",
  "EXPIRE",
  "EXPIREAT",
  "PEXPIRE",
  "PEXPIREAT",
  "PERSIST",
  "RENAME",
  "RENAMENX",
  "SET",
  "SETEX",
  "PSETEX",
  "SETNX",
  "MSET",
  "MSETNX",
  "HSET",
  "HDEL",
  "LPUSH",
  "RPUSH",
  "LPOP",
  "RPOP",
  "LSET",
  "LREM",
  "SADD",
  "SREM",
  "ZADD",
  "ZREM",
  "XADD",
  "XDEL",
  "FLUSHDB",
]);

export function firstRedisCommandToken(commandText: string): string | undefined {
  try {
    return parseRedisCommandArgv(commandText)[0]?.toUpperCase();
  } catch {
    const token = commandText.trim().match(/^\S+/)?.[0]?.toUpperCase();
    return token || undefined;
  }
}

export function classifyRedisCommand(commandText: string): RedisCommandSafety {
  const command = firstRedisCommandToken(commandText);
  if (!command) return "blocked";
  if (BLOCKED_REDIS_COMMANDS.has(command)) return "blocked";
  if (CONFIRM_REDIS_COMMANDS.has(command)) return "confirm";
  return "allowed";
}

export function evaluateRedisCommandSafety(commandText: string, options: SqlSafetyOptions = {}): RedisCommandSafetyDecision {
  const command = firstRedisCommandToken(commandText);
  if (!command) {
    return { allowed: false, reason: "Redis command is empty." };
  }

  const safety = classifyRedisCommand(command);
  if (safety === "blocked" && !options.allowDangerous) {
    return {
      allowed: false,
      command,
      safety,
      reason: `Dangerous Redis command "${command}" is blocked. Set DBX_MCP_ALLOW_DANGEROUS_SQL=1 to allow it.`,
    };
  }

  if (safety !== "allowed" && !options.allowWrites) {
    return {
      allowed: false,
      command,
      safety,
      reason: "MCP Redis command execution is read-only for this session. Set DBX_MCP_ALLOW_WRITES=1 to allow write or dangerous commands.",
    };
  }

  return {
    allowed: true,
    command,
    safety,
    skipSafetyCheck: safety === "blocked" && options.allowDangerous === true,
  };
}

export function parseRedisCommandArgv(commandText: string): string[] {
  const trimmed = commandText.trimEnd().replace(/;+$/, "");
  const argv: string[] = [];
  let current = "";
  let quote: "\"" | "'" | undefined;
  let escaping = false;

  for (const ch of trimmed) {
    if (escaping) {
      if (ch === "n") current += "\n";
      else if (ch === "r") current += "\r";
      else if (ch === "t") current += "\t";
      else current += ch;
      escaping = false;
      continue;
    }

    if (ch === "\\") {
      escaping = true;
      continue;
    }

    if (quote) {
      if (ch === quote) quote = undefined;
      else current += ch;
      continue;
    }

    if (ch === "\"" || ch === "'") {
      quote = ch;
      continue;
    }

    if (/\s/.test(ch)) {
      if (current) {
        argv.push(current);
        current = "";
      }
      continue;
    }

    current += ch;
  }

  if (escaping) current += "\\";
  if (quote) throw new Error("Redis command has an unterminated quote");
  if (current) argv.push(current);
  if (argv.length === 0) throw new Error("Redis command is empty");
  return argv;
}
