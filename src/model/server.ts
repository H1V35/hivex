import { spawn, spawnSync } from "node:child_process";
import { AppServerConnection } from "./connection.ts";
import { ServerAdmissionFailure } from "./failure.ts";
import {
  admitProfile,
  launchArguments,
  nativeEnvironment,
  nativeVersion,
  requestedPolicyHash,
} from "./profile.ts";

const signalGroup = (
  pid: number,
  signal: Parameters<typeof process.kill>[1]
) => {
  try {
    process.kill(-pid, signal);
    return true;
  } catch (error) {
    if (Error.isError(error) && "code" in error && error.code === "ESRCH") {
      return false;
    }
    if (
      signal === 0 &&
      Error.isError(error) &&
      "code" in error &&
      error.code === "EPERM"
    ) {
      return true;
    }
    throw error;
  }
};

const areOnlyTerminatedMembersRemaining = (pid: number) => {
  if (process.platform !== "linux") {
    return false;
  }
  const result = spawnSync("/bin/ps", ["-e", "-o", "pgid=,stat="], {
    encoding: "utf-8",
    env: Object.fromEntries([
      ...Object.entries(nativeEnvironment()),
      ["LC_ALL", "C"],
    ]),
    maxBuffer: 1_048_576,
    timeout: 1000,
  });
  if (result.status !== 0) {
    return false;
  }
  const rows = result.stdout.trim().split("\n");
  if (rows.some((row) => !/^\s*\d+\s+\S+\s*$/u.test(row))) {
    return false;
  }
  const members = rows
    .map((row) => row.trim().split(/\s+/u))
    .filter(([group]) => Number(group) === pid);
  if (members.length === 0) {
    return !signalGroup(pid, 0);
  }
  // Container init may retain orphan zombies; they cannot execute or receive signals.
  return members.every(
    ([, state]) =>
      state?.startsWith("Z") === true || state?.startsWith("X") === true
  );
};

const areGroupMembersTerminated = async (pid: number) => {
  const deadline = performance.now() + 2000;
  const result = Promise.withResolvers<boolean>();
  const check = (): void => {
    if (!signalGroup(pid, 0)) {
      result.resolve(true);
      return;
    }
    if (performance.now() >= deadline) {
      result.resolve(areOnlyTerminatedMembersRemaining(pid));
      return;
    }
    setTimeout(check, 25);
  };
  check();
  return await result.promise;
};

const isExitComplete = async (exit: Promise<unknown>) => {
  await exit;
  return true;
};

const isSettledWithin = async (
  exit: Promise<unknown>,
  milliseconds: number
) => {
  const expired = Promise.withResolvers<boolean>();
  const timer = setTimeout(() => {
    expired.resolve(false);
  }, milliseconds);
  try {
    return await Promise.race([isExitComplete(exit), expired.promise]);
  } finally {
    clearTimeout(timer);
  }
};

interface ServerOptions {
  binary: string;
  workspace: string;
  notification: (method: string, parameters: unknown) => void;
  interaction: (method: string) => void;
  signal: AbortSignal;
}

const captureFailure = async <Value>(promise: Promise<Value>) => {
  try {
    return { value: await promise };
  } catch (error) {
    return { error };
  }
};

const terminateProcessGroup = async (pid: number, exit: Promise<unknown>) => {
  await isSettledWithin(exit, 1000);
  const isTerminated = signalGroup(pid, "SIGTERM");
  if (isTerminated) {
    const areMembersTerminated = await areGroupMembersTerminated(pid);
    if (!areMembersTerminated) {
      signalGroup(pid, "SIGKILL");
      if (!(await areGroupMembersTerminated(pid))) {
        throw new Error("Owned Codex process group survived cleanup");
      }
    }
  }
  if (!(await isSettledWithin(exit, 1000))) {
    throw new Error("Native Codex process was not reaped");
  }
};

const launchServer = async (
  options: ServerOptions,
  disabledServers: string[]
) => {
  const version = spawnSync(options.binary, ["--version"], {
    encoding: "utf-8",
    env: nativeEnvironment(),
    maxBuffer: 65_536,
    timeout: 10_000,
  });
  if (version.status !== 0 || version.stdout.trim() !== nativeVersion) {
    throw new Error("Knowledge execution requires verified codex-cli 0.153.2");
  }
  const child = spawn(options.binary, launchArguments(disabledServers), {
    cwd: options.workspace,
    detached: true,
    env: nativeEnvironment(),
    stdio: "pipe",
  });
  const exit = Promise.withResolvers<boolean>();
  child.once("exit", () => {
    exit.resolve(true);
  });
  child.once("error", () => {
    exit.resolve(true);
  });
  child.stderr.resume();
  const { pid } = child;
  if (pid === undefined) {
    throw new Error("Native Codex could not start");
  }
  const rpc = new AppServerConnection({
    input: child.stdin,
    onInteractiveRequest: options.interaction,
    onNotification: options.notification,
    output: child.stdout,
  });
  const stop = async () => {
    child.stdin.end();
    const result = await captureFailure(
      terminateProcessGroup(pid, exit.promise)
    );
    rpc.dispose();
    child.stdin.destroy();
    child.stdout.destroy();
    child.stderr.destroy();
    if ("error" in result) {
      throw result.error;
    }
  };
  try {
    await rpc.request(
      "initialize",
      { clientInfo: { name: "hivex", version: "0.1.0" } },
      { signal: options.signal }
    );
    rpc.notify("initialized");
    const profile = await admitProfile({ rpc, signal: options.signal });
    return {
      activeServers: profile.activeServers,
      admission: {
        ...profile.evidence,
        launchPolicyHash: requestedPolicyHash(disabledServers),
      },
      pid,
      rpc,
      stop,
    };
  } catch (error) {
    try {
      await stop();
    } catch {
      throw new ServerAdmissionFailure({
        cause: error,
        cleanup: "failed",
        processId: pid,
      });
    }
    throw new ServerAdmissionFailure({
      cause: error,
      cleanup: "confirmed",
      processId: pid,
    });
  }
};

const stopForAdmission = async (
  server: Awaited<ReturnType<typeof launchServer>>
) => {
  try {
    await server.stop();
  } catch (error) {
    throw new ServerAdmissionFailure({
      admission: server.admission,
      cause: error,
      cleanup: "failed",
      processId: server.pid,
    });
  }
};

export const startServer = async (options: ServerOptions) => {
  const initial = await launchServer(options, []);
  if (initial.activeServers.length === 0) {
    return initial;
  }
  await stopForAdmission(initial);
  if (options.signal.aborted) {
    throw new ServerAdmissionFailure({
      admission: initial.admission,
      cause: options.signal.reason,
      cleanup: "confirmed",
      processId: initial.pid,
    });
  }
  const isolated = await launchServer(options, initial.activeServers);
  if (isolated.activeServers.length === 0) {
    return isolated;
  }
  await stopForAdmission(isolated);
  throw new ServerAdmissionFailure({
    admission: isolated.admission,
    cause: new Error(
      "MCP configuration changed or did not honor process-local overrides"
    ),
    cleanup: "confirmed",
    processId: isolated.pid,
  });
};
