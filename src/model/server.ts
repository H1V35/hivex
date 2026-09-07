import { spawn, spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { AppServerConnection } from './connection.ts';
import { ServerAdmissionFailure } from './failure.ts';
import {
  admitProfile,
  launchArguments,
  nativeEnvironment,
  nativeVersion,
  requestedPolicyHash,
} from './profile.ts';

function signalGroup(pid: number, signal: NodeJS.Signals | 0) {
  try {
    process.kill(-pid, signal);
    return true;
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ESRCH') return false;
    if (signal === 0 && error instanceof Error && 'code' in error && error.code === 'EPERM')
      return true;
    throw error;
  }
}

async function groupExited(pid: number) {
  const deadline = performance.now() + 2000;
  while (signalGroup(pid, 0)) {
    if (performance.now() >= deadline) return false;
    await delay(25);
  }
  return true;
}

async function settledWithin(exit: Promise<unknown>, milliseconds: number) {
  const expired = Promise.withResolvers<boolean>();
  const timer = setTimeout(() => expired.resolve(false), milliseconds);
  try {
    return await Promise.race([exit.then(() => true), expired.promise]);
  } finally {
    clearTimeout(timer);
  }
}

type ServerOptions = {
  binary: string;
  workspace: string;
  notification: (method: string, params: unknown) => void;
  interaction: (method: string) => void;
  signal: AbortSignal;
};

async function launchServer(options: ServerOptions, disabledServers: string[]) {
  const version = spawnSync(options.binary, ['--version'], {
    env: nativeEnvironment(),
    encoding: 'utf8',
    timeout: 10_000,
    maxBuffer: 65_536,
  });
  if (version.status !== 0 || version.stdout.trim() !== nativeVersion)
    throw new Error('Knowledge execution requires verified codex-cli 0.153.2');
  const child = spawn(options.binary, launchArguments(disabledServers), {
    env: nativeEnvironment(),
    cwd: options.workspace,
    stdio: 'pipe',
    detached: true,
  });
  const exit = Promise.withResolvers<void>();
  child.once('exit', () => exit.resolve());
  child.once('error', () => exit.resolve());
  child.stderr.resume();
  if (child.pid === undefined) throw new Error('Native Codex could not start');
  const pid = child.pid;
  const rpc = new AppServerConnection({
    input: child.stdin,
    output: child.stdout,
    onNotification: options.notification,
    onInteractiveRequest: options.interaction,
  });
  const stop = async () => {
    child.stdin.end();
    try {
      await settledWithin(exit.promise, 1000);
      if (signalGroup(pid, 'SIGTERM') && !(await groupExited(pid))) {
        signalGroup(pid, 'SIGKILL');
        if (!(await groupExited(pid)))
          throw new Error('Owned Codex process group survived cleanup');
      }
      if (!(await settledWithin(exit.promise, 1000)))
        throw new Error('Native Codex process was not reaped');
    } finally {
      rpc.dispose();
      child.stdin.destroy();
      child.stdout.destroy();
      child.stderr.destroy();
    }
  };
  try {
    await rpc.request(
      'initialize',
      { clientInfo: { name: 'hivex', version: '0.1.0' } },
      { signal: options.signal },
    );
    rpc.notify('initialized');
    const profile = await admitProfile({ rpc, signal: options.signal });
    return {
      rpc,
      stop,
      pid,
      activeServers: profile.activeServers,
      admission: { ...profile.evidence, launchPolicyHash: requestedPolicyHash(disabledServers) },
    };
  } catch (error) {
    try {
      await stop();
    } catch {
      throw new ServerAdmissionFailure({ cause: error, cleanup: 'failed', processId: pid });
    }
    throw new ServerAdmissionFailure({ cause: error, cleanup: 'confirmed', processId: pid });
  }
}

export async function startServer(options: ServerOptions) {
  const initial = await launchServer(options, []);
  if (!initial.activeServers.length) return initial;
  await stopForAdmission(initial);
  if (options.signal.aborted)
    throw new ServerAdmissionFailure({
      cause: options.signal.reason,
      cleanup: 'confirmed',
      processId: initial.pid,
      admission: initial.admission,
    });
  const isolated = await launchServer(options, initial.activeServers);
  if (!isolated.activeServers.length) return isolated;
  await stopForAdmission(isolated);
  throw new ServerAdmissionFailure({
    cause: new Error('MCP configuration changed or did not honor process-local overrides'),
    cleanup: 'confirmed',
    processId: isolated.pid,
    admission: isolated.admission,
  });
}

async function stopForAdmission(server: Awaited<ReturnType<typeof launchServer>>) {
  try {
    await server.stop();
  } catch (error) {
    throw new ServerAdmissionFailure({
      cause: error,
      cleanup: 'failed',
      processId: server.pid,
      admission: server.admission,
    });
  }
}
