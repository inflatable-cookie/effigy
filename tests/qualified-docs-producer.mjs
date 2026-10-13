// Genuine private producer fixture. Approval is embedded by its pinned program.
import path from 'node:path';
import { promises as fs } from 'node:fs';
import assert from 'node:assert/strict';
import { adapterLease } from '/Users/tom/Dev/projects/nucleus/hosts/paseo/dist/src/adapter-lease.js';
import { readControl } from '/Users/tom/Dev/projects/nucleus/hosts/paseo/dist/src/maintenance.js';
import { DocsInputController, allocateDocsInputs, lookupDocsInputs } from '/Users/tom/Dev/projects/nucleus/hosts/paseo/dist/src/docs-input-provider.js';
import { prepareQualifiedDocs, executeQualifiedDocs, preflightQualifiedDocs } from '../scripts/qualified-docs-execution.mjs';

export async function fixture(root, approved) {
  const release = await adapterLease(root);
  const file = path.join(root, 'adapter-owner.json'), stat = await fs.stat(file, { bigint: true });
  const lease = { owner: await readControl(file), incarnation: `${stat.dev}:${stat.ino}:${stat.ctimeNs}:${stat.size}` };
  process.once('message', async frame => {
    try {
      assert.equal(frame.type, 'bind');
      const controller = await DocsInputController.attach(frame.peer, approved);
      process.send({ version: 1, type: 'bound', nonce: frame.peer.nonce });
      const packet = await allocateDocsInputs(controller);
      if (approved.binding.effect === 'expired') {
        await new Promise(r => setTimeout(r, Math.max(0, approved.deadline - Date.now() + 20)));
        await assert.rejects(prepareQualifiedDocs(controller, packet), /original deadline/);
        await fs.writeFile(path.join(root, 'result.json'), JSON.stringify({ unavailable: 'original deadline', leafEffects: 0 }), { mode: 0o600, flag: 'wx' });
        return;
      }
      if (approved.binding.effect === 'shadow') {
        await assert.rejects(prepareQualifiedDocs(controller, packet), /resolution refused/);
        await fs.writeFile(path.join(root, 'result.json'), JSON.stringify({ unavailable: 'named source shadow', leafEffects: 0 }), { mode: 0o600, flag: 'wx' });
        return;
      }
      const receipt = await prepareQualifiedDocs(controller, packet);
      assert.throws(() => receipt.resolution.leaves[0].argv.push(';'), TypeError);
      await assert.rejects(executeQualifiedDocs(controller, packet, structuredClone(receipt)), /original in-process/);
      await assert.rejects(preflightQualifiedDocs({ context: approved }, packet, receipt), /installed controller/);
      for (const candidate of [
        { ...packet, installationEpoch: 'drift' },
        { ...packet, resource: { ...packet.resource, deadline: 1 } },
        { ...packet, resource: { ...packet.resource, generation: 'drift' } },
        { ...packet, controllerDigest: 'drift' },
        { ...packet, contextDigest: 'drift' },
        { ...packet, sourceInventoryDigest: 'drift' },
        { ...packet, environment: { ...packet.environment, NODE_OPTIONS: '--require drift' } },
        { ...packet, environment: { ...packet.environment, HOME: '/Users/tom' } },
        { ...packet, environment: { ...packet.environment, PATH: '/usr/bin' } },
        { ...packet, command: { ...packet.command, argv: [...packet.command.argv, '--json'] } },
        { ...packet, inputs: packet.inputs.map((i, n) => n ? i : { ...i, sha256: '0'.repeat(64) }) },
        { ...packet, tools: packet.tools.map((i, n) => n ? i : { ...i, canonical: '/usr/bin/false' }) },
      ]) await assert.rejects(preflightQualifiedDocs(controller, candidate, receipt));
      // No negative above launches a leaf or consumes a full retained-byte check.
      const result = await executeQualifiedDocs(controller, packet, receipt);
      await assert.rejects(executeQualifiedDocs(controller, packet, receipt), /original in-process/);
      assert.ok(result.observations.every(o => o.pid > 0));
      if (approved.binding.effect === 'timeout') {
        assert.equal(result.code, 2); assert.equal(result.observations.at(-1).signal, 'SIGKILL');
        assert.equal(result.observations.at(-1).failure, 'owned invocation deadline');
      } else assert.ok(result.observations.every(o => o.signal === null && !o.failure));
      const source = path.join(packet.resource.root, 'source');
      await fs.writeFile(path.join(source, 'undeclared.md'), 'outside declared inventory', { mode: 0o600, flag: 'wx' });
      const drift = await lookupDocsInputs(controller, packet);
      assert.equal(drift.state, 'held');
      assert.match(drift.reason, /namespace\/configuration changed/);
      const script = path.join(packet.resource.root, 'skill/scripts/retired-concepts.ts');
      await fs.appendFile(script, '\n// private drift negative\n');
      const scriptDrift = await lookupDocsInputs(controller, packet);
      assert.equal(scriptDrift.state, 'held'); assert.match(scriptDrift.reason, /retained input changed/);
      await fs.appendFile(path.join(root, 'producer.mjs'), '\n// private program drift negative\n');
      const programDrift = await lookupDocsInputs(controller, packet);
      assert.equal(programDrift.state, 'held'); assert.match(programDrift.reason, /changed adapter program/);
      await fs.writeFile(path.join(root, 'result.json'), JSON.stringify({ result, negatives: 17, drift, scriptDrift, programDrift }), { mode: 0o600, flag: 'wx' });
    } catch (e) {
      await fs.writeFile(path.join(root, 'held.json'), JSON.stringify({ reason: String(e), stack: e.stack }), { mode: 0o600, flag: 'wx' });
      process.exitCode = 1;
    } finally { await release(); process.disconnect(); }
  });
  process.send({ version: 1, type: 'ready', store: approved.binding.store, lease });
}
