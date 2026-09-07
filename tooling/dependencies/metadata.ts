import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { digest } from './lock.ts';

const maximumBytes = 64 * 1024 * 1024;

async function fetchPackage(name: string, signal: AbortSignal) {
  const url = `https://registry.npmjs.org/${encodeURIComponent(name)}`;
  const response = await fetch(url, {
    headers: { accept: 'application/json' },
    redirect: 'error',
    signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
  });
  if (!response.ok || !response.body)
    throw new Error(`Registry metadata unavailable: ${name} (${response.status})`);
  const chunks: Uint8Array[] = [];
  const reader = response.body.getReader();
  let bytes = 0;
  while (true) {
    const result = await reader.read();
    if (result.done) break;
    const chunk: unknown = result.value;
    if (!(chunk instanceof Uint8Array)) throw new Error('Unexpected metadata stream chunk');
    bytes += chunk.byteLength;
    if (bytes > maximumBytes) {
      await reader.cancel();
      throw new Error(`Registry metadata exceeds ${maximumBytes} bytes: ${name}`);
    }
    chunks.push(chunk);
  }
  const text = new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks));
  const value: unknown = JSON.parse(text);
  return {
    value,
    text,
    url,
    bytes,
    sha256: digest(text),
    fetchedAt: new Date().toISOString(),
  };
}

export async function downloadMetadata(names: string[]) {
  const directory = await mkdtemp(join(tmpdir(), 'hivex-dependencies-'));
  const unique = [...new Set(names)].sort();
  const packuments = new Map<string, unknown>();
  const evidence: {
    name: string;
    file: string;
    url: string;
    bytes: number;
    sha256: string;
    fetchedAt: string;
  }[] = [];
  const cancellation = new AbortController();
  const failures: unknown[] = [];
  let next = 0;
  const worker = async () => {
    try {
      while (next < unique.length && !cancellation.signal.aborted) {
        const name = unique[next++];
        if (!name) throw new Error('Missing planned package identity');
        const result = await fetchPackage(name, cancellation.signal);
        const file = `${digest(name)}.json`;
        await writeFile(join(directory, file), result.text, { mode: 0o600 });
        packuments.set(name, result.value);
        evidence.push({
          name,
          file,
          url: result.url,
          bytes: result.bytes,
          sha256: result.sha256,
          fetchedAt: result.fetchedAt,
        });
      }
    } catch (error) {
      failures.push(error);
      cancellation.abort();
    }
  };
  await Promise.all(Array.from({ length: Math.min(4, unique.length) }, worker));
  evidence.sort((a, b) => a.name.localeCompare(b.name, 'en'));
  await writeFile(join(directory, 'manifest.json'), JSON.stringify(evidence, null, 2) + '\n', {
    mode: 0o600,
  });
  if (failures.length)
    throw new Error(`Metadata download failed; partial evidence: ${directory}`, {
      cause: failures[0],
    });
  return { directory, packuments, evidence };
}
