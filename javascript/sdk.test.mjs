import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { errorInfo, failure } from "./sdk.mjs";

const context = {
  plugin_id: "example", capability_id: "search",
  operation: "search", request_id: "request-1",
};

test("plugin failures retain the complete protocol envelope", () => {
  const details = { reason: "upstream_busy" };
  const result = failure("network_error", "Upstream busy", context, {
    details, retryable: true,
  });
  details.reason = "mutated";
  assert.deepEqual(result, { ok: false, error: {
    code: "network_error", message: "Upstream busy",
    details: { reason: "upstream_busy" }, retryable: true, ...context,
  } });
});

test("declared error codes match runtime accepted error codes", () => {
  const declaration = readFileSync(new URL("./sdk.d.ts", import.meta.url), "utf8");
  const union = declaration.split("export type PluginErrorCode =")[1].split(";")[0];
  const codes = [...union.matchAll(/"([a-z_]+)"/g)].map((match) => match[1]);
  assert.equal(codes.length, 16);
  for (const code of codes) assert.equal(failure(code, "Failure", context).error.code, code);
  assert.throws(() => failure("arbitrary", "Failure", context), TypeError);
});

test("invalid identities and oversized UTF-8 messages fail before transport", () => {
  assert.throws(() => failure("invalid_input", "书".repeat(171), context), TypeError);
  assert.throws(() => failure("invalid_input", "", context), TypeError);
  assert.throws(() => failure("invalid_input", "Failure", {}), TypeError);
  assert.throws(() => failure("invalid_input", "Failure", context, {
    details: "书".repeat(1366),
  }), TypeError);
  assert.throws(() => failure("invalid_input", "Failure", context, {
    retryable: "false",
  }), TypeError);
});

test("partial failures preserve Host classification and copy diagnostic details", () => {
  const error = Object.assign(new Error("Request failed"), {
    code: "timeout", details: { operation: "http.request" }, retryable: true,
  });
  const info = errorInfo(error);
  error.details.operation = "changed";
  assert.deepEqual(info, {
    code: "timeout", message: "Request failed",
    details: { operation: "http.request" }, retryable: true,
  });
  assert.deepEqual(errorInfo({ pluginError: info }), info);
});

test("error capture stays serializable for unknown, cyclic and oversized errors", () => {
  const error = Object.assign(new Error("书".repeat(200)), { code: "invented" });
  error.details = error;
  const info = errorInfo(error, "parse_error");
  assert.equal(info.code, "parse_error");
  assert.ok(Buffer.byteLength(info.message) <= 512);
  assert.equal(info.details, null);
  assert.equal(info.retryable, false);
  assert.equal(errorInfo("Failure").message, "Failure");
  assert.equal(errorInfo(null).message, "Plugin operation failed");
  assert.throws(() => errorInfo(error, "invented"), TypeError);
});
