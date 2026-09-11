import { createSessionApi } from './session.mjs';

let cvc5;
let verifier;
async function backend() {
  cvc5 ??= import('./cvc5.mjs').then(({ default: initialize }) => initialize()).then(createSessionApi);
  return cvc5;
}
async function rustVerifier() {
  verifier ??= import('./solver_web.js').then(async (module) => {
    await module.default();
    return module;
  });
  return verifier;
}
const check = (condition, message) => {
  if (!condition) throw new Error(message);
};
function rejects(action, message) {
  let threw = false;
  try {
    action();
  } catch {
    threw = true;
  }
  check(threw, message);
}

async function sessions() {
  const api = await backend();
  const a = api.createSession();
  const b = api.createSession();
  const owned = [a, b];
  try {
    a.execute('(set-logic QF_LRA) (declare-fun x () Real) (assert (= x (/ 1 3)))');
    b.execute('(set-logic QF_LRA) (declare-fun x () Real) (assert (= x 2))');
    check(a.execute('(check-sat)') === 'sat', 'first session must be SAT');
    const model = a.execute('(get-value (x))');
    check(/\(\(x \(\/ 1(?:\.0)? 3(?:\.0)?\)\)\)/.test(model), 'model must retain exact 1/3: ' + model);
    a.execute('(assert (not (= x (/ 1 3))))');
    check(a.execute('(check-sat)') === 'unsat', 'blocking must exhaust the first session');
    check(b.execute('(check-sat)') === 'sat', 'second session must remain independent');
    check(b.execute('(get-value (x))').includes('2'), 'second session must retain its own symbol');
    check(b.execute('(push 1) (assert false) (check-sat)') === 'unsat', 'push must retain incremental state');
    check(b.execute('(pop 1) (check-sat)') === 'sat', 'pop must restore incremental state');
    const enumeration = api.createSession();
    owned.push(enumeration);
    enumeration.execute(
      '(set-logic QF_LRA) (declare-fun choice () Bool) (declare-fun rate () Real) (assert (= rate (ite choice (/ 1 3) (/ 2 3))))',
    );
    const seen = new Set();
    for (let index = 0; index < 2; index++) {
      check(enumeration.execute('(check-sat)') === 'sat', 'both enumerated models must exist');
      const model = enumeration.execute('(get-value (choice rate))');
      const choice = /\(choice (true|false)\)/.exec(model)?.[1];
      check(choice && !seen.has(choice), 'model blocker must remove the previous assignment');
      check(model.includes('(/'), 'enumeration must return exact rational rates');
      seen.add(choice);
      enumeration.execute(`(assert (not (= choice ${choice})))`);
    }
    check(enumeration.execute('(check-sat)') === 'unsat', 'all models must exhaust after both blockers');
    const broken = api.createSession();
    owned.push(broken);
    broken.execute('(set-logic QF_LRA)');
    rejects(() => broken.execute('(assert undeclared)'), 'malformed input must fail');
    rejects(() => broken.execute('(check-sat)'), 'failed session must not be reused');
    check(b.execute('(check-sat)') === 'sat', 'parser error must not poison another session');
    const commandError = api.createSession();
    owned.push(commandError);
    commandError.execute('(set-logic QF_LRA) (declare-fun x () Real)');
    rejects(() => commandError.execute('(get-value (x))'), 'command errors must not be treated as output');
    const limited = api.createSession({ resourceLimit: 1 });
    owned.push(limited);
    limited.execute('(set-logic QF_LRA) (declare-fun x () Real) (assert (> x 0))');
    const unknown = limited.execute('(check-sat)');
    check(unknown === 'unknown', 'resource exhaustion must remain unknown, got ' + unknown);
    const reason = limited.execute('(get-info :reason-unknown)');
    a.dispose();
    a.dispose();
    rejects(() => a.execute('(check-sat)'), 'disposed handle must be rejected');
    for (let index = 0; index < 16; index++) {
      const session = api.createSession();
      try {
        check(session.execute('(set-logic QF_LRA) (assert true) (check-sat)') === 'sat', 'fresh session must run');
      } finally {
        session.dispose();
      }
    }
    return { model, unknown, reason, enumeratedModels: seen.size, recreated: 16, heapBytes: api.heapBytes() };
  } finally {
    owned.forEach((session) => session.dispose());
    check(api.activeSessions() === 0, 'all sessions must be released');
  }
}

async function busy(id) {
  const api = await backend();
  const session = api.createSession();
  const pigeons = 40;
  const holes = pigeons - 1;
  const statements = ['(set-logic QF_LRA)'];
  for (let p = 0; p < pigeons; p++) {
    for (let h = 0; h < holes; h++) statements.push(`(declare-fun p${p}h${h} () Bool)`);
    statements.push(`(assert (or ${Array.from({ length: holes }, (_, h) => `p${p}h${h}`).join(' ')}))`);
  }
  for (let h = 0; h < holes; h++) {
    for (let a = 0; a < pigeons; a++) {
      for (let b = a + 1; b < pigeons; b++) statements.push(`(assert (not (and p${a}h${h} p${b}h${h})))`);
    }
  }
  try {
    session.execute(statements.join('\n'));
    postMessage({ id, kind: 'checking' });
    return session.execute('(check-sat)');
  } finally {
    session.dispose();
  }
}

self.onmessage = async ({ data }) => {
  try {
    let value;
    if (data.operation === 'verify') {
      const module = await rustVerifier();
      const fixtures = await (await fetch('./fixtures.json')).json();
      value = fixtures.map((fixture) => ({
        name: fixture.name,
        actual: JSON.parse(
          (fixture.operation === 'verify' ? module.verify_witness_json : module.reconstruct_topology_json)(
            fixture.payload,
          ),
        ),
        native: fixture.native,
      }));
    } else if (data.operation === 'coexist') {
      const module = await rustVerifier();
      const fixtures = await (await fetch('./fixtures.json')).json();
      const before = module.verify_witness_json(fixtures[0].payload);
      const api = await backend();
      const session = api.createSession();
      try {
        const answer = session.execute(
          '(set-logic QF_LRA) (declare-fun x () Real) (assert (= x (/ 1 3))) (check-sat) (get-value (x))',
        );
        const after = module.verify_witness_json(fixtures[0].payload);
        check(before === after, 'cvc5 execution must not change Rust verifier memory');
        value = { verified: JSON.parse(after).kind, answer };
      } finally {
        session.dispose();
      }
    } else if (data.operation === 'sessions') {
      value = await sessions();
    } else if (data.operation === 'busy') {
      value = await busy(data.id);
    } else if (data.operation === 'ping') {
      const api = await backend();
      const session = api.createSession();
      try {
        value = session.execute('(set-logic QF_LRA) (assert true) (check-sat)');
      } finally {
        session.dispose();
      }
    } else {
      throw new Error('unknown test operation');
    }
    postMessage({ id: data.id, kind: 'result', value });
  } catch (error) {
    postMessage({ id: data.id, kind: 'error', error: String(error) });
  }
};
