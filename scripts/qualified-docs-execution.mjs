/** Exact three-leaf bridge for the reviewed, cooperative macOS private provider.
 * No task JSON installs a controller. No native admission or durable outcome is issued.
 */
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { lookupDocsInputs } from '/Users/tom/Dev/projects/nucleus/hosts/paseo/dist/src/docs-input-provider.js';

const prepared = new WeakMap();
const executed = new WeakSet();
const originalPreparations = new Set();
const names = ['qa:docs:links', 'qa:docs:paths', 'qa:docs:retired'];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sorted = v => Array.isArray(v) ? v.map(sorted) : v && typeof v === 'object' ?
  Object.fromEntries(Object.keys(v).sort().map(k => [k, sorted(v[k])])) : v;
const same = (a, b) => JSON.stringify(sorted(a)) === JSON.stringify(sorted(b));
const freeze = v => { if (v && typeof v === 'object') { Object.values(v).forEach(freeze); Object.freeze(v); } return v; };
const fail = reason => { throw Error(`qualified docs unavailable: ${reason}`); };

// Only this bounded docs bridge uses the child runner. It is not a command API.
async function owned(command, budget, deadline) {
  if (++budget.commands > 4 || performance.now() - budget.started >= 60_000 || Date.now() >= deadline) fail('original execution budget');
  const limit = Math.min(15_000, deadline - Date.now(), 60_000 - (performance.now() - budget.started));
  return await new Promise((resolve, reject) => {
    const child = spawn(command.binary, command.argv, { cwd: command.cwd, env: command.env,
      detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '', total = 0, active = true, failure;
    const started = performance.now();
    const terminate = reason => {
      failure ??= reason;
      if (active && child.pid) {
        try { process.kill(-child.pid, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') failure = String(e); }
      }
    };
    const timer = setTimeout(() => terminate('owned invocation deadline'), limit);
    const capture = key => b => {
      total += b.length;
      if (total > 1024 * 1024) terminate('owned output budget');
      else if (key === 'stdout') stdout += b; else stderr += b;
    };
    child.stdout.on('data', capture('stdout')); child.stderr.on('data', capture('stderr'));
    child.once('error', e => { active = false; clearTimeout(timer); reject(e); });
    child.once('exit', () => { active = false; clearTimeout(timer); }); // Never signal a reaped owner.
    child.once('close', (code, signal) => {
      clearTimeout(timer);
      resolve({ binary: command.binary, argv: command.argv, cwd: command.cwd, pid: child.pid,
        stdout, stderr, code, signal, failure, ms: performance.now() - started });
    });
  });
}

/** Run authoritative NAMED selection in preparation, before private binding.
 * `approved` comes from the pinned private producer program, not a task request.
 * The returned object is an in-process receipt; serialization loses it.
 */
export async function prepareQualifiedDocs(controller, packet) {
  const authenticated = await lookupDocsInputs(controller, packet);
  if (authenticated.state !== 'available') fail(`live preparation held: ${authenticated.reason}`);
  const original = `${authenticated.packet.resource.root}:${authenticated.packet.resource.incarnation}:${authenticated.packet.contextDigest}`;
  if (originalPreparations.has(original)) fail('original preparation already consumed');
  originalPreparations.add(original);
  const approved = controller.context;
  if (process.platform !== 'darwin' || approved.version !== 4 || approved.policy_id !== 'bounded-host-runtime-docs-source-lock-v1' ||
      approved.namedSkill?.canonical !== '/Users/tom/.agents/skills/northstar') fail('unsupported approved preparation context');
  const tool = approved.tools.find(t => t.name === 'effigy');
  if (!tool) fail('missing private Effigy');
  for (const name of ['bun', 'git']) {
    if (approved.tools.find(t => t.name === name)?.canonical !== await fs.realpath(`/opt/homebrew/bin/${name}`)) fail('unsupported named tool binding');
  }
  // Live lookup already verifies the executable. Reserve the resolver's own
  // executable/source reads and pinned-program checks in the same byte ceiling.
  const inputBytes = approved.inputs.reduce((n, i) => n + i.bytes, 0);
  const readBytes = inputBytes + approved.tools.reduce((n, t) => n + t.bytes, 0);
  const programBytes = (await fs.stat(controller.peer.spec.program)).size;
  const additionalReads = tool.bytes + 8 * inputBytes + 32 * programBytes + 65536;
  const reservedControls = 32 * (packet.limits.packetBytes + 8192);
  if (4 * readBytes + additionalReads + reservedControls > packet.limits.verificationBytes) fail('complete verification read budget');
  const budget = { commands: 0, started: performance.now() };
  const observation = await owned({ binary: tool.canonical, argv: ['--json', 'docs', 'qualified', 'resolve'],
    cwd: approved.sourceRoot, env: { HOME: '/Users/tom', PATH: '', LANG: 'C', LC_ALL: 'C', TZ: 'UTC' } }, budget, approved.deadline);
  if (observation.failure || observation.signal || observation.code !== 0) fail(`resolution refused (${observation.code}/${observation.signal})`);
  const envelope = JSON.parse(observation.stdout);
  const resolution = envelope.result;
  if (resolution?.schema !== 'effigy.docs.qualified.resolution.v1' || resolution.version !== 1 ||
      resolution.target !== approved.sourceRoot || resolution.named_source.canonical !== approved.namedSkill.canonical ||
      !same(resolution.leaves.map(l => l.task), names) || !same(resolution.effigy, toolWithoutName(tool))) fail('resolution binding mismatch');
  // Freeze the complete original read inventory against the constructor context.
  for (const input of resolution.source_files) {
    const expected = approved.inputs.find(i => i.relative === `source/${input.relative}`);
    if (!expected || input.canonical !== expected.canonical || input.bytes !== expected.bytes || input.sha256 !== expected.sha256) fail('source inventory mismatch');
  }
  if (resolution.source_files.length !== approved.inputs.filter(i => i.relative.startsWith('source/')).length ||
      !same(resolution.source_directories, [...approved.directories].sort())) fail('incomplete source inventory');
  for (const asset of resolution.named_source.assets) {
    const expected = approved.inputs.find(i => i.relative === `skill/${asset.relative}`);
    if (!expected || asset.canonical !== expected.canonical || asset.bytes !== expected.bytes || asset.sha256 !== expected.sha256) fail('named selection mismatch');
  }
  const receipt = freeze({ schema: 'effigy.docs.qualified.prepared.v1', resolution });
  prepared.set(receipt, { contextHash: hash(JSON.stringify(approved)), approved: structuredClone(approved), budget, observation, additionalReads });
  return receipt;
}

function toolWithoutName(tool) {
  return { canonical: tool.canonical, bytes: tool.bytes, sha256: tool.sha256 };
}

/** Execute only this process's authoritative preparation using the actual live
 * provider controller. A copied receipt, packet, boolean or fake controller fails.
 */
async function bindQualifiedDocs(controller, packet, receipt) {
  const state = prepared.get(receipt);
  if (!state || executed.has(receipt)) fail('original in-process preparation required');
  if (hash(JSON.stringify(controller?.context)) !== state.contextHash || packet.version !== 4 ||
      packet.policy_id !== state.approved.policy_id || packet.installationEpoch !== state.approved.installationEpoch ||
      !same(packet.binding, state.approved.binding) || !same(packet.namedSkill, state.approved.namedSkill) ||
      packet.resource.deadline !== state.approved.deadline || !same(packet.recipe.leaves, names)) fail('controller/epoch/lifetime binding mismatch');
  const root = packet.resource.root, source = path.join(root, 'source'), skill = path.join(root, 'skill');
  const effigy = packet.tools.find(t => t.name === 'effigy'), bun = packet.tools.find(t => t.name === 'bun');
  if (!same(toolWithoutName(effigy), toolWithoutName(state.approved.tools.find(t => t.name === 'effigy')))) fail('Effigy binding drift');
  const retired = { binary: bun.canonical, argv: ['run', '--no-install', '--no-env-file', '--no-macros',
    path.join(skill, 'scripts/retired-concepts.ts'), '--repo', source], cwd: skill };
  if (!same(packet.command, retired)) fail('retired literal argv binding mismatch');
  if (!same(packet.writes, ['home', 'temp', 'output'].map(n => path.join(root, n))) ||
      packet.environment.PATH !== path.join(root, 'bin') || packet.environment.HOME !== path.join(root, 'home') ||
      packet.environment.TMPDIR !== path.join(root, 'temp') || Object.keys(packet.environment).some(k => /^(NODE_OPTIONS|BUN_OPTIONS|npm_|NPM_|ENV$|BASH_ENV$)/.test(k))) fail('environment/write binding mismatch');
  const lookup = await lookupDocsInputs(controller, packet);
  if (lookup.state !== 'available') fail(`live provider held: ${lookup.reason}`);
  return { state, root, source, retired, effigy, packet: lookup.packet };
}

export async function preflightQualifiedDocs(controller, packet, receipt) {
  const binding = await bindQualifiedDocs(controller, packet, receipt);
  return Object.freeze({ schema: 'effigy.docs.qualified.preflight.v1', resource: binding.packet.resource,
    authority: 'observation-only-requires-live-execution-lookup' });
}

export async function executeQualifiedDocs(controller, packet, receipt) {
  // This preflight is also the first leaf's live authorization. No extra full
  // byte check is spent between this and its launch.
  const binding = await bindQualifiedDocs(controller, packet, receipt);
  executed.add(receipt);
  const { state, root, source, retired, effigy } = binding;
  const observations = [];
  for (const [index, leaf] of receipt.resolution.leaves.entries()) {
    const lookup = index === 0 ? { state: 'available', packet: binding.packet } : await lookupDocsInputs(controller, packet);
    if (lookup.state !== 'available') fail(`live provider held: ${lookup.reason}`);
    // The authoritative original bytes must equal the retained private inputs.
    for (const input of [...receipt.resolution.source_files.map(i => ({ ...i, relative: `source/${i.relative}` })),
      ...receipt.resolution.named_source.assets.map(i => ({ ...i, relative: `skill/${i.relative}` }))]) {
      const retained = lookup.packet.inputs.find(i => i.relative === input.relative);
      if (!retained || retained.bytes !== input.bytes || retained.sha256 !== input.sha256 ||
          retained.retained !== path.join(root, input.relative)) fail('retained source binding drift');
    }
    const command = leaf.task === 'qa:docs:retired' ? retired : {
      binary: effigy.canonical, argv: leaf.argv.slice(1), cwd: source,
    };
    const observed = await owned({ ...command, env: lookup.packet.environment }, state.budget, packet.resource.deadline);
    observed.writeObservations = [];
    for (const directory of packet.writes) {
      const stat = await fs.lstat(directory);
      if (!stat.isDirectory() || await fs.realpath(directory) !== directory) fail('write namespace changed');
      const entries = await fs.readdir(directory);
      observed.writeObservations.push({ directory, entries });
      if (entries.length) fail('unsupported cache/temp/output effect');
    }
    observations.push({ task: leaf.task, ...observed });
    if (observed.failure || observed.signal || observed.code !== 0) break;
  }
  const last = observations.at(-1);
  if (Date.now() >= packet.resource.deadline || performance.now() - state.budget.started >= 60_000) fail('original result deadline');
  return { schema: 'effigy.docs.qualified.execution.v1', version: 1, resource: packet.resource,
    installationEpoch: packet.installationEpoch, selected: receipt.resolution,
    preparation: state.observation, observations, code: last?.failure || last?.signal ? 2 : last?.code ?? 2,
    commands: state.budget.commands, accepted: packet.accepted, verified: packet.verified,
    declarations: { tools: packet.tools, inputs: packet.inputs, directories: packet.directories,
      namedSkill: packet.namedSkill, environment: packet.environment, reads: packet.reads,
      writes: packet.writes, providerControls: packet.providerControls, sourceInventoryDigest: packet.sourceInventoryDigest },
    limits: packet.limits, additionalVerificationReadsReserved: state.additionalReads, nativeAuthorization: 'unavailable', durableOutcome: 'unavailable' };
}
