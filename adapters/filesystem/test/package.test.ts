import { expect, test } from "bun:test";
import pkg from "../package.json";

test("filesystem adapter uses Bun for tests and exposes a build script", () => {
  expect(pkg.scripts.test).toContain("bun test");
  expect(pkg.scripts.build).toContain("tsc");
});
