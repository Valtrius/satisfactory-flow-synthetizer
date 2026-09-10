//! Port quotient: unary belt multiplicities, one exact output rate per operator.
use crate::profile::AccountedProfile;
use crate::{cardinality::exactly, process::Failure};
use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeType,
    OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef,
    Rational,
};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Clone, Copy)]
pub(crate) enum Counts {
    Sparse,
    Boolean,
}

impl Counts {
    fn exactly(self, script: &mut String, prefix: &str, literals: &[String], count: usize) {
        match self {
            Self::Boolean => exactly(script, prefix, literals, count),
            Self::Sparse => {
                let terms = literals
                    .iter()
                    .map(|s| format!("(ite {s} 1 0)"))
                    .collect::<Vec<_>>();
                writeln!(script, "(assert (= {} {count}))", sum(&terms)).unwrap();
            }
        }
    }
}

struct Edge {
    source: usize,
    target: usize,
    name: String,
}

pub struct Encoding {
    pub script: String,
    edges: Vec<Edge>,
    nodes: Vec<NodeType>,
    inputs: usize,
    outputs: usize,
}

fn sum(terms: &[String]) -> String {
    match terms {
        [] => "0".into(),
        [one] => one.clone(),
        _ => format!("(+ {})", terms.join(" ")),
    }
}
fn either(terms: &[String]) -> String {
    match terms {
        [] => "false".into(),
        [one] => one.clone(),
        _ => format!("(or {})", terms.join(" ")),
    }
}
fn rate(value: &Rational) -> String {
    format!("(/ {} {})", value.numerator(), value.denominator())
}

impl Encoding {
    #[allow(clippy::too_many_lines)]
    pub fn new(problem: &Problem, task: AccountedProfile, counts_encoding: Counts) -> Self {
        let p = task.profile;
        let mut nodes = Vec::new();
        for (kind, count) in [
            (NodeType::Splitter2, p.splitter2),
            (NodeType::Splitter3, p.splitter3),
            (NodeType::Merger2, p.merger2),
            (NodeType::Merger3, p.merger3),
        ] {
            nodes.extend(std::iter::repeat_n(kind, count as usize));
        }
        let inputs = problem.inputs.len();
        let outputs = problem.outputs.len();
        let sources = inputs + nodes.len();
        let discard = outputs + nodes.len();
        let targets = discard + usize::from(task.accounting.discard_link_count > 0);
        let mut script = "(set-logic QF_LRA)\n".to_owned();
        let mut edges = Vec::new();
        let flows: Vec<_> = problem
            .inputs
            .iter()
            .map(rate)
            .chain((0..nodes.len()).map(|n| format!("f{n}")))
            .collect();
        for (n, kind) in nodes.iter().enumerate() {
            writeln!(
                script,
                "(declare-fun f{n} () Real)\n(assert (> f{n} 0))\n(assert (<= (* {} f{n}) {}))",
                kind.output_port_count(),
                rate(&problem.max_link_rate)
            )
            .unwrap();
            writeln!(
                script,
                "(declare-fun a{n} () Real)\n(declare-fun z{n} () Real)"
            )
            .unwrap();
            if n > 0 && nodes[n - 1] == *kind {
                writeln!(script, "(assert (<= f{} f{n}))", n - 1).unwrap();
            }
        }
        let mut rows = vec![Vec::new(); sources];
        let mut columns = vec![Vec::new(); targets];
        let mut incoming = vec![Vec::new(); targets];
        let counts: Vec<_> = (0..targets)
            .map(|target| {
                if target == discard {
                    task.accounting.discard_link_count
                } else if target < outputs {
                    1
                } else {
                    u32::from(nodes[target - outputs].input_port_count())
                }
            })
            .collect();
        let required_flows: Vec<_> = (0..targets)
            .map(|target| {
                if target == discard {
                    rate(&problem.surplus().expect("nonnegative surplus"))
                } else if target < outputs {
                    rate(&problem.outputs[target])
                } else {
                    format!(
                        "(* {} f{})",
                        nodes[target - outputs].output_port_count(),
                        target - outputs
                    )
                }
            })
            .collect();
        let mut predecessors = vec![Vec::new(); nodes.len()];
        let mut successors = vec![Vec::new(); nodes.len()];
        let mut terminal_links = Vec::new();
        for source in 0..sources {
            let arity = if source < inputs {
                1
            } else {
                nodes[source - inputs].output_port_count()
            };
            for target in 0..targets {
                if source >= inputs && target >= outputs && source - inputs == target - outputs {
                    continue;
                }
                let width = if target == discard {
                    arity
                } else if target < outputs {
                    1
                } else {
                    arity.min(nodes[target - outputs].input_port_count())
                };
                for copy in 0..width {
                    let name = format!("e{}", edges.len());
                    writeln!(script, "(declare-fun {name} () Bool)").unwrap();
                    if copy > 0 {
                        writeln!(script, "(assert (=> {name} e{}))", edges.len() - 1).unwrap();
                    }
                    rows[source].push(name.clone());
                    columns[target].push(name.clone());
                    if counts[target] == 1 {
                        // Exactly one edge is active in this column. Its source
                        // rate equals the required flow; a conditional sum adds
                        // no constraint. This applies to outputs, splitters and
                        // a discard destination with exactly one physical belt.
                        writeln!(
                            script,
                            "(assert (=> {name} (= {} {})))",
                            flows[source], required_flows[target]
                        )
                        .unwrap();
                    } else {
                        incoming[target].push(format!("(ite {name} {} 0)", flows[source]));
                    }
                    if source < inputs && (target < outputs || target == discard) {
                        terminal_links.push(name.clone());
                    }
                    if copy == 0 {
                        if target >= outputs && target < discard {
                            let n = target - outputs;
                            predecessors[n].push(if source < inputs {
                                name.clone()
                            } else {
                                format!("(and {name} (< a{} a{n}))", source - inputs)
                            });
                        }
                        if source >= inputs {
                            let n = source - inputs;
                            successors[n].push(if target < outputs || target == discard {
                                name.clone()
                            } else {
                                format!("(and {name} (< z{} z{n}))", target - outputs)
                            });
                        }
                    }
                    edges.push(Edge {
                        source,
                        target,
                        name,
                    });
                }
            }
            counts_encoding.exactly(
                &mut script,
                &format!("r{source}"),
                &rows[source],
                usize::from(arity),
            );
        }
        for target in 0..targets {
            let count = counts[target];
            counts_encoding.exactly(
                &mut script,
                &format!("c{target}"),
                &columns[target],
                usize::try_from(count).unwrap(),
            );
            if count != 1 {
                writeln!(
                    script,
                    "(assert (= {} {}))",
                    sum(&incoming[target]),
                    required_flows[target]
                )
                .unwrap();
            }
        }
        for n in 0..nodes.len() {
            writeln!(
                script,
                "(assert {})\n(assert {})",
                either(&predecessors[n]),
                either(&successors[n])
            )
            .unwrap();
        }
        // Every operator input is fed by an operator or an external input.
        // Every external input feeds an operator, requested output or discard.
        // Thus L = operator input ports - external inputs + direct terminal belts.
        // Counting the last (usually much smaller) set is exactly equivalent to
        // counting all operator-to-operator belts, given the row/column equations.
        let direct = i128::from(task.accounting.link_count) + i128::try_from(inputs).unwrap()
            - nodes
                .iter()
                .map(|kind| i128::from(kind.input_port_count()))
                .sum::<i128>();
        if direct < 0 {
            writeln!(script, "(assert false)").unwrap();
        } else {
            counts_encoding.exactly(
                &mut script,
                "d",
                &terminal_links,
                usize::try_from(direct).unwrap(),
            );
        }
        Self {
            script,
            edges,
            nodes,
            inputs,
            outputs,
        }
    }

    /// Each requested output has exactly one incoming belt. Fixing its source
    /// partitions all Boolean models into disjoint, collectively exhaustive roots.
    pub fn output_source_assertion(&self, output: usize, source: usize) -> Result<String, Failure> {
        let edge = self
            .edges
            .iter()
            .find(|edge| edge.target == output && edge.source == source)
            .ok_or_else(|| Failure::Worker("missing Solver output partition edge".into()))?;
        Ok(format!("(assert {})\n", edge.name))
    }

    pub fn query(&self) -> String {
        format!(
            "(get-value ({}))\n",
            self.edges
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }

    pub fn model(&self, response: &str) -> Result<(PhysicalGraph, String), Failure> {
        let tokens: Vec<_> = response
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .filter(|s| !s.is_empty())
            .collect();
        let mut values = BTreeMap::new();
        if tokens.len() != self.edges.len() * 2 {
            return Err(Failure::Worker("malformed cvc5 model".into()));
        }
        for pair in tokens.chunks_exact(2) {
            let value = match pair[1] {
                "true" => true,
                "false" => false,
                _ => return Err(Failure::Worker("non-Boolean edge model".into())),
            };
            if values.insert(pair[0], value).is_some() {
                return Err(Failure::Worker("duplicate model variable".into()));
            }
        }
        let mut graph = PhysicalGraph {
            nodes: self
                .nodes
                .iter()
                .enumerate()
                .map(|(i, &node_type)| PhysicalNode {
                    id: NodeId(u32::try_from(i).unwrap()),
                    node_type,
                })
                .collect(),
            links: Vec::new(),
        };
        let mut producers = vec![0; self.inputs + self.nodes.len()];
        let mut consumers = vec![0; self.outputs + self.nodes.len()];
        let mut discards = 0;
        let mut block = Vec::new();
        for edge in &self.edges {
            let active = *values
                .get(edge.name.as_str())
                .ok_or_else(|| Failure::Worker("missing model variable".into()))?;
            block.push(if active {
                format!("(not {})", edge.name)
            } else {
                edge.name.clone()
            });
            if !active {
                continue;
            }
            let producer = if edge.source < self.inputs {
                ProducerPortRef::Input(InputTerminalIndex(u32::try_from(edge.source).unwrap()))
            } else {
                ProducerPortRef::Node {
                    node: NodeId(u32::try_from(edge.source - self.inputs).unwrap()),
                    port: producers[edge.source],
                }
            };
            producers[edge.source] += 1;
            let consumer = if edge.target == consumers.len() {
                let id = discards;
                discards += 1;
                ConsumerPortRef::Discard(DiscardTerminalIndex(id))
            } else if edge.target < self.outputs {
                ConsumerPortRef::Output(OutputTerminalIndex(u32::try_from(edge.target).unwrap()))
            } else {
                ConsumerPortRef::Node {
                    node: NodeId(u32::try_from(edge.target - self.outputs).unwrap()),
                    port: consumers[edge.target],
                }
            };
            if edge.target < consumers.len() {
                consumers[edge.target] += 1;
            }
            graph.links.push(PhysicalLink {
                producer,
                consumer,
                flow: Rational::zero(),
            });
        }
        Ok((graph, format!("(assert {})\n", either(&block))))
    }
}
