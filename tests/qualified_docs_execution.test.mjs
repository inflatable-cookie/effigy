import test from 'node:test';
import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createHash, randomUUID } from 'node:crypto';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createMaintenanceSpec } from '/Users/tom/Dev/projects/nucleus/hosts/paseo/dist/src/maintenance.js';

const repo = path.resolve(import.meta.dirname, '..');
const nucleus = '/Users/tom/Dev/projects/nucleus';
const pin = async file => {
  const canonical = await fs.realpath(file), data = await fs.readFile(canonical);
  return { canonical, bytes: data.length, sha256: createHash('sha256').update(data).digest('hex') };
};
const identity = s => `${s.dev}:${s.ino}:${s.birthtimeMs}`;

for (const mode of ['clean', 'links-fail', 'retired-fail', 'shadow', 'timeout', 'expired']) test(`qualified docs genuine private ${mode}`, async () => {
  assert.equal(process.platform, 'darwin', 'this qualified fixture requires the approved macOS host');
  assert.equal((await promisify(execFile)('/opt/homebrew/bin/git', ['rev-parse', 'HEAD'], { cwd: nucleus })).stdout.trim(),
    'd36cd4af22e78ea2b76ff8d0175e66ddab655841');
  const root = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'effigy-qualified-')));
  await fs.chmod(root, 0o700);
  const binary = path.join(root, 'effigy');
  await fs.copyFile(path.join(repo, 'target/qualified-docs/effigy'), binary);
  // The ordinary targeted compile produces the size-bounded private artifact.
  const sourceRoot = path.join(root, 'fixture-source');
  const directories = ['.git', '.git/objects', '.git/refs', 'docs', 'docs/knowledge'];
  for (const d of directories) await fs.mkdir(path.join(sourceRoot, d), { recursive: true, mode: 0o700 });
  const manifest = '[catalog]\nalias="private"\n[manifest]\ninclude=["tasks.toml"]\n';
  const tasks = '[tasks]\n"qa:docs"=[{task="qa:docs:links"},{task="qa:docs:paths"},{task="qa:docs:retired"}]\n' +
    '"qa:docs:links"="effigy docs check links"\n"qa:docs:paths"="effigy docs check paths README.md docs docs/knowledge/retired.toml"\n' +
    '"qa:docs:retired"="effigy skill run northstar/retired-concepts"\n';
  const files = {
    'effigy.toml': manifest, 'tasks.toml': tasks,
    'README.md': mode === 'links-fail' ? '[missing](docs/missing.md)\n' : '[owner](docs/owner.md)\n',
    'docs/owner.md': 'gone-proof-fixture\n',
    'ordinary.txt': mode === 'retired-fail' ? 'gone-proof-fixture\n' : 'current concept\n',
    'docs/knowledge/retired.toml': '[[retired]]\nid="fixture"\nterms=["gone-proof-fixture"]\nowner="docs/owner.md"\n',
    '.git/HEAD': 'ref: refs/heads/main\n',
    '.git/config': '[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n\tignorecase = true\n\tprecomposeunicode = true\n',
  };
  if (mode === 'timeout') files['docs/knowledge/retired.toml'] = '[[retired]]\nid="bounded-timeout"\nterms=' + JSON.stringify(Array.from({ length: 10000 }, (_, n) => `absent-bounded-${n}`)) + '\n';
  if (mode === 'shadow') {
    directories.push('.agents', '.agents/skills', '.agents/skills/northstar');
    await fs.mkdir(path.join(sourceRoot, '.agents/skills/northstar'), { recursive: true, mode: 0o700 });
    for (const name of ['SKILL.md', 'effigy.toml']) files[`.agents/skills/northstar/${name}`] = await fs.readFile(`/Users/tom/.agents/skills/northstar/${name}`, 'utf8');
  }
  for (const [name, data] of Object.entries(files)) await fs.writeFile(path.join(sourceRoot, name), data, { mode: 0o600, flag: 'wx' });
  const inputs = [];
  for (const name of Object.keys(files)) inputs.push({ ...await pin(path.join(sourceRoot, name)), relative: `source/${name}` });
  for (const name of ['SKILL.md', 'effigy.toml', 'scripts/retired-concepts.ts']) inputs.push({ ...await pin(`/Users/tom/.agents/skills/northstar/${name}`), relative: `skill/${name}` });
  const tools = [];
  for (const [name, file] of [['bun', '/opt/homebrew/bin/bun'], ['git', '/opt/homebrew/bin/git'], ['effigy', binary]]) tools.push({ name, ...await pin(file) });
  const readBytes = [...inputs, ...tools].reduce((n, i) => n + i.bytes, 0);
  assert.ok(Math.floor((512 * 1024 ** 2 - 32 * (1024 ** 2 + 8192)) / readBytes) >= 4,
    `root=${root} readBytes=${readBytes}: preparation and three live retained checks must fit original byte budget`);
  const skill = '/Users/tom/.agents/skills/northstar', marker = await fs.lstat(path.join(skill, 'SKILL.md'));
  const approved = { version: 4, policy_id: 'bounded-host-runtime-docs-source-lock-v1',
    namedSkill: { canonical: skill, identity: identity(await fs.lstat(skill)),
      markerIdentity: `${identity(marker)}:${marker.size}:${marker.ctimeMs}:${marker.mtimeMs}:${marker.nlink}` },
    installationEpoch: 'effigy153-private-reviewed-installation',
    binding: { store: 'synthetic:effigy-qualified-docs', plane: 'private', task: '153', run: randomUUID(), effect: mode,
      brief: 'v4', head: 'private-under-test', base: 'da114170', settings: 'unchanged', candidate: sourceRoot,
      sourceGeneration: randomUUID(), resourceGeneration: randomUUID() }, sourceRoot, directories, inputs, tools,
    deadline: Date.now() + 60_000,
    recipe: { manifestSha256: inputs.find(i => i.relative === 'source/effigy.toml').sha256,
      leaves: ['qa:docs:links', 'qa:docs:paths', 'qa:docs:retired'] } };
  const program = path.join(root, 'producer.mjs'), spec = path.join(root, 'control.json');
  // The protected program contains the actual Effigy-owned executor and fixture
  // code. Its authenticated program hash covers both, not a mutable import.
  const bridge = await fs.readFile(path.join(repo, 'scripts/qualified-docs-execution.mjs'), 'utf8');
  const producer = (await fs.readFile(path.join(repo, 'tests/qualified-docs-producer.mjs'), 'utf8'))
    .replace("import path from 'node:path';\n", '')
    .replace("import { promises as fs } from 'node:fs';\n", '')
    .replace('DocsInputController, allocateDocsInputs, lookupDocsInputs', 'DocsInputController, allocateDocsInputs')
    .replace("import { prepareQualifiedDocs, executeQualifiedDocs, preflightQualifiedDocs } from '../scripts/qualified-docs-execution.mjs';\n", '');
  await fs.writeFile(program, `${bridge}\n${producer}\nawait fixture(${JSON.stringify(root)}, ${JSON.stringify(approved)});`, { mode: 0o600, flag: 'wx' });
  await createMaintenanceSpec(spec, program, root, approved.binding.store);
  const child = spawn(process.execPath, [path.join(nucleus, 'experiments/paseo-live/canary-process.mjs'), path.join(root, 'exit.json'),
    '--adapter-maintenance-v1', spec, process.execPath, program], { env: {}, stdio: ['ignore', 'ignore', 'pipe'] });
  let errors = ''; child.stderr.on('data', b => { if (errors.length < 65536) errors += b; });
  let active = true;
  child.once('exit', () => { active = false; });
  const closed = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => { active = false; resolve({ code, signal }); }); });
  const watchdog = setTimeout(() => { if (active) child.kill('SIGINT'); }, 70_000);
  const exit = await closed; clearTimeout(watchdog);
  const held = await fs.readFile(path.join(root, 'held.json'), 'utf8').catch(e => e.code === 'ENOENT' ? undefined : Promise.reject(e));
  assert.equal(held, undefined, `root=${root} stderr=${errors}`);
  assert.deepEqual(exit, { code: 0, signal: null }, `root=${root} stderr=${errors}`);
  const evidence = JSON.parse(await fs.readFile(path.join(root, 'result.json'), 'utf8'));
  if (mode === 'expired' || mode === 'shadow') {
    assert.equal(evidence.leafEffects, 0);
    assert.equal(evidence.unavailable, mode === 'expired' ? 'original deadline' : 'named source shadow');
    console.log(JSON.stringify({ mode, root, evidence, originalAllocationRetained: true }));
    return;
  }
  const result = evidence.result;
  assert.equal(result.schema, 'effigy.docs.qualified.execution.v1');
  assert.equal(result.code, mode === 'clean' ? 0 : mode === 'timeout' ? 2 : 1);
  assert.deepEqual(result.observations.map(o => o.task), mode === 'links-fail' ? ['qa:docs:links'] : approved.recipe.leaves);
  assert.equal(result.commands, mode === 'links-fail' ? 2 : 4);
  assert.match(result.observations[0].stdout + result.observations[0].stderr, mode === 'links-fail' ? /target does not exist|broken|missing/i : /link check passed/);
  if (mode !== 'links-fail') {
    assert.match(result.observations[1].stdout, /docs path check passed/);
    if (mode !== 'timeout') assert.match(result.observations[2].stdout, mode === 'clean' ? /retired-concepts: OK \(1 retired concept\)/ : /retired-concepts: 1 live reference/);
  }
  assert.equal(result.nativeAuthorization, 'unavailable'); assert.equal(result.durableOutcome, 'unavailable');
  const owned = JSON.parse(await fs.readFile(path.join(root, 'exit.json'), 'utf8'));
  assert.equal(owned.code, 0); assert.equal(owned.signal, null);
  assert.equal(evidence.drift.state, 'held');
  assert.equal(evidence.scriptDrift.state, 'held'); assert.equal(evidence.programDrift.state, 'held');
  const schema = JSON.parse(await fs.readFile(path.join(repo, 'docs/knowledge/contracts/qualified-docs-resolution-v1.schema.json'), 'utf8'));
  assert.deepEqual(Object.keys(result.selected).sort(), [...schema.required].sort());
  assert.deepEqual(result.selected.leaves.map(l => l.ast.surface), ['docs-check', 'docs-check', 'named-skill']);
  console.log(JSON.stringify({ mode, root, readBytes, tool: tools.find(t => t.name === 'effigy'), commands: result.commands,
    leaves: result.observations.map(o => ({ task: o.task, binary: o.binary, argv: o.argv, code: o.code, signal: o.signal, ms: o.ms })),
    negativeControls: evidence.negatives, originalAllocationRetained: true, accepted: result.accepted }));
});
