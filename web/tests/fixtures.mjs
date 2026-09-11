import fs from 'node:fs';

const node = (id, nodeType) => ({ id, nodeType });
const port = (owner, value, number = 0) => ({ owner, port: owner === 'node' ? { node: value, port: number } : value });
const link = (producer, consumer, flow) => ({ producer, consumer, flow });
const input = (index) => port('input', index);
const output = (index) => port('output', index);
const operator = (index, number = 0) => port('node', index, number);
const discard = (index) => port('discard', index);
const endpoints = (rates, prefix) =>
  rates.map((rate, index) => ({ id: `${prefix}${index}`, name: `${prefix} ${index}`, rate }));
const request = (inputs, outputs, beltRate) => ({
  inputs: endpoints(inputs, 'i'),
  outputs: endpoints(outputs, 'o'),
  beltRate,
});
const direct = (rate = '1/3') => ({
  request: request([], [rate], '1200'),
  graph: { nodes: [], links: [link(input(0), output(0), rate)] },
});
const clone = (value) => JSON.parse(JSON.stringify(value));

export function verificationFixtures() {
  const result = [];
  function add(name, data, expectedKind = 'verified', operation = 'verify') {
    result.push({ name, payload: typeof data === 'string' ? data : JSON.stringify(data), expectedKind, operation });
  }
  function bad(name, edit) {
    const value = direct();
    edit(value);
    add(name, value, 'rejected');
  }
  add('exact one-third and automatic supply', direct());
  add('finite decimal', direct('0.0000000000000000001'));
  const large = direct('123456789012345678901234567890123456789/7');
  large.request.beltRate = '999999999999999999999999999999999999999999';
  add('arbitrary precision beyond JavaScript Number', large);
  add('split at capacity', {
    request: request(['120'], ['60', '60'], '120'),
    graph: {
      nodes: [node(0, 'splitter2')],
      links: [
        link(input(0), operator(0), '120'),
        link(operator(0), output(0), '60'),
        link(operator(0, 1), output(1), '60'),
      ],
    },
  });
  add('explicit multiple inputs', {
    request: request(['30', '90'], ['120'], '120'),
    graph: {
      nodes: [node(7, 'merger2')],
      links: [
        link(input(0), operator(7), '30'),
        link(input(1), operator(7, 1), '90'),
        link(operator(7), output(0), '120'),
      ],
    },
  });
  add('multiple anonymous discard belts', {
    request: request(['120'], ['40'], '120'),
    graph: {
      nodes: [node(0, 'splitter3')],
      links: [
        link(input(0), operator(0), '120'),
        link(operator(0), output(0), '40'),
        link(operator(0, 1), discard(0), '40'),
        link(operator(0, 2), discard(1), '40'),
      ],
    },
  });
  const feedback = {
    request: request(['5'], ['10/3', '5/3'], '20'),
    graph: {
      nodes: [node(0, 'merger2'), node(1, 'splitter2'), node(2, 'splitter2')],
      links: [
        link(input(0), operator(0), '5'),
        link(operator(2), operator(0, 1), '5/3'),
        link(operator(0), operator(1), '20/3'),
        link(operator(1), operator(2), '10/3'),
        link(operator(1, 1), output(0), '10/3'),
        link(operator(2, 1), output(1), '5/3'),
      ],
    },
  };
  add('unique feedback SCC', feedback);
  const topology = clone(feedback);
  topology.graph.links.forEach((edge) => delete edge.flow);
  add('recover feedback flows without supplied flow or coordinates', topology, 'verified', 'reconstruct');
  const feedbackDiscard = clone(feedback);
  feedbackDiscard.request.outputs.pop();
  feedbackDiscard.graph.links[5].consumer = discard(0);
  add('feedback with surplus discard', feedbackDiscard);
  add(
    'singular circulating component',
    {
      request: request(['1'], ['1'], '10'),
      graph: {
        nodes: [node(0, 'splitter2'), node(1, 'merger2')],
        links: [
          link(input(0), output(0), '1'),
          link(operator(0), operator(1), '1'),
          link(operator(0, 1), operator(1, 1), '1'),
          link(operator(1), operator(0), '2'),
        ],
      },
    },
    'rejected',
  );
  bad('wrong supplied flow', (value) => {
    value.graph.links[0].flow = '1/2';
  });
  bad('zero supplied flow', (value) => {
    value.graph.links[0].flow = '0';
  });
  bad('invalid denominator', (value) => {
    value.request.outputs[0].rate = '1/0';
  });
  bad('nonexistent node reference', (value) => {
    value.graph.links[0].consumer = operator(99);
  });
  bad('duplicate physical port', (value) => {
    value.graph.links.push(clone(value.graph.links[0]));
  });
  bad('excess capacity', (value) => {
    value.request.beltRate = '1/4';
  });
  bad('rate allocation guard', (value) => {
    value.graph.links[0].flow = '9'.repeat(257);
  });
  bad('proof cannot be imported', (value) => {
    value.proof = { minimumNodeCount: 0 };
  });
  bad('missing exact witness flow', (value) => {
    delete value.graph.links[0].flow;
  });
  add('topology rejects supplied flows', direct(), 'rejected', 'reconstruct');
  add('malformed JSON', '{', 'rejected');
  add('payload allocation guard', ' '.repeat(262_145), 'rejected');
  const corpus = JSON.parse(
    fs.readFileSync(new URL('../../crates/solver-core/tests/fixtures/preferred-order.json', import.meta.url), 'utf8'),
  );
  corpus.graphs.forEach((graph, index) =>
    add(`native seven-operator fixture ${index + 1}`, {
      request: request(corpus.problem.inputs, corpus.problem.outputs, corpus.problem.maxLinkRate),
      graph,
    }),
  );
  return result;
}
