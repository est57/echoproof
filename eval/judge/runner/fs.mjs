// File access for the runner. The flow directory can be influenced by model
// output, so every read and write refuses symlinks, hard links and paths that
// resolve outside the flow root.
import { closeSync, constants as FS, fstatSync, ftruncateSync, lstatSync, mkdirSync, openSync, readFileSync, realpathSync, writeFileSync, writeSync } from 'node:fs';
import { dirname } from 'node:path';

export const WIN = process.platform === 'win32';
const NOFOLLOW = WIN ? 0 : FS.O_NOFOLLOW;

const ESC_SEQ = /\x1b\[[0-?]*[ -\/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)?|\x1b[@-_]/g;
const CONTROL = /[\x00-\x1f\x7f-\x9f]/g;
const termSafe = s => String(s).replace(ESC_SEQ, '').replace(CONTROL, '');
export const eprint = (...a) => console.error(...a.map(termSafe));

/** Only "no such entry" counts as absent; any other lstat failure is rethrown. */
export const lstatOrNull = p => {
  try { return lstatSync(p); } catch (e) { if (e?.code === 'ENOENT') return null; throw e; }
};
export const isSymlink = p => lstatOrNull(p)?.isSymbolicLink() === true;
export const lexists = p => lstatOrNull(p) != null;

let flowRealRoot = null;
export function setFlowRoot(dir) {
  flowRealRoot = realpathSync(dir);
}

function assertInFlow(dir, what) {
  if (flowRealRoot == null) throw new Error(`refusing to ${what}: flow root not resolved yet`);
  const real = realpathSync(dir);
  if (real !== flowRealRoot && !real.startsWith(flowRealRoot + (WIN ? '\\' : '/')))
    throw new Error(`refusing to ${what}: ${dir} resolves outside the flow directory`);
}

function openNoFollow(p, flags) {
  if (isSymlink(dirname(p))) throw new Error(`refusing to open through symlinked directory: ${dirname(p)}`);
  assertInFlow(dirname(p), 'open');
  if (WIN && isSymlink(p)) throw new Error(`refusing to open through symlink: ${p}`);
  const fd = openSync(p, flags | NOFOLLOW, 0o644);
  try {
    const st = fstatSync(fd);
    if (!st.isFile()) throw new Error(`refusing to use non-regular file: ${p}`);
    if (st.nlink > 1) throw new Error(`refusing to use ${p}: it has a second hard link`);
  } catch (e) { closeSync(fd); throw e; }
  return fd;
}

/** Truncates only after the open checks pass, so a refused file keeps its bytes. */
export function writeFileNoFollow(p, data) {
  const fd = openNoFollow(p, FS.O_WRONLY | FS.O_CREAT);
  try { ftruncateSync(fd, 0); writeFileSync(fd, data); } finally { closeSync(fd); }
}

export function appendFileNoFollow(p, data) {
  const fd = openNoFollow(p, FS.O_WRONLY | FS.O_CREAT | FS.O_APPEND);
  try {
    if (!WIN) { writeFileSync(fd, data); return; }
    const buf = Buffer.from(data);
    const start = fstatSync(fd).size;
    for (let off = 0; off < buf.length;) {
      const n = writeSync(fd, buf, off, buf.length - off, start + off);
      if (n <= 0) throw new Error(`append to ${p} made no progress`);
      off += n;
    }
  } finally { closeSync(fd); }
}

export function readFileNoFollow(p) {
  const fd = openNoFollow(p, FS.O_RDONLY);
  try { return readFileSync(fd, 'utf8'); } finally { closeSync(fd); }
}

/** null when absent; any other failure (a planted link included) throws. */
export function readIfPresent(p) {
  try { return readFileNoFollow(p); } catch (e) { if (e?.code === 'ENOENT') return null; throw e; }
}

/** Checked after creating: recursive mkdir follows symlinked ancestors. */
export function mkdirNoFollow(dir) {
  if (isSymlink(dir)) throw new Error(`refusing to use symlinked directory: ${dir}`);
  mkdirSync(dir, { recursive: true });
  assertInFlow(dir, 'create directory');
}
