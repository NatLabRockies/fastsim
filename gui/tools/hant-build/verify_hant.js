// Verify hant.js against the build-time pipeline, string by string.
// usage: node verify_hant.js <pairs.json>
const fs = require('fs');
const path = require('path');

const global_ = {};
const src = fs.readFileSync(path.join(__dirname, '..', 'frontend', 'hant.js'), 'utf-8');
new Function('window', src)(global_);
const toHant = global_.FSHant.toHant;

const pairs = JSON.parse(fs.readFileSync(process.argv[2], 'utf-8'));
let bad = 0;
for (const [zh, want] of Object.entries(pairs)) {
  const got = toHant(zh);
  if (got !== want) {
    bad += 1;
    if (bad <= 15) console.log(`MISMATCH\n  src  ${zh}\n  got  ${got}\n  want ${want}`);
  }
}
// idempotence on plain ASCII and on already-traditional text
const samples = ['FASTSim Studio', 'SOC [%]', 'UDDS（城市）', '——', '—', ''];
for (const s of samples) {
  const once = toHant(s);
  const twice = toHant(once);
  if (once !== twice) {
    bad += 1;
    console.log(`NOT IDEMPOTENT: ${s} -> ${once} -> ${twice}`);
  }
}
console.log(`checked ${Object.keys(pairs).length} strings, ${bad} problem(s)`);
console.log(`tables: ${global_.FSHant.charCount} chars, ${global_.FSHant.phraseCount} phrases`);
process.exit(bad === 0 ? 0 : 1);
