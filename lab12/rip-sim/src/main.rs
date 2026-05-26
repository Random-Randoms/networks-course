use anyhow::{Context, anyhow, bail};
use clap::Parser;
use toml::{Table, Value};

use std::{collections::HashMap, iter};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    config: String,

    #[arg(long, action)]
    show_intermediate: bool,
}

/// Structural representation of network graph
struct Graph {
    nodes: HashMap<String, usize>,
    adj: Vec<Vec<bool>>,
}

impl Graph {
    fn from_toml(mut toml: Table) -> anyhow::Result<Self> {
        let nodes = match toml.remove("hosts").ok_or(anyhow!("No `hosts` field"))? {
            Value::Array(nodes) => nodes
                .into_iter()
                .map(|v| match v {
                    Value::String(host) => Ok(host),
                    _ => Err(anyhow!("`hosts` field elements must be strings")),
                })
                .enumerate()
                .map(|(i, h)| h.map(|hh| (hh, i)))
                .collect::<anyhow::Result<HashMap<String, usize>>>()?,
            _ => bail!("`hosts` field must have array type"),
        };
        let mut adj = Vec::from_iter(iter::repeat_n(
            Vec::from_iter(iter::repeat_n(false, nodes.len())),
            nodes.len(),
        ));
        match toml.remove("channels").ok_or(anyhow!("No `channels` field"))? {
            Value::Array(edges) => {
                edges
                    .into_iter()
                    .try_for_each(|v| {
                        match v {
                            Value::Table(edge) => {
                                let from = match edge.get("from").ok_or(anyhow!("`channels` element must have `from` field"))? {
                                    Value::String(host) => nodes
                                        .get(host)
                                        .ok_or(anyhow!("`from` field of `channels` element must be a host, declared in `hosts` field"))?,
                                    _ => return Err(anyhow!("`from` field of `channels` element must be string")),
                                };
                                let to  = match edge.get("to").ok_or(anyhow!("`channels` element must have `to` field"))? {
                                    Value::String(host) => nodes
                                        .get(host)
                                        .ok_or(anyhow!("`to` field of `channels` element must be a host, declared in `hosts` field"))?,
                                    _ => return Err(anyhow!("`to` field of `channels` element must be string")),
                                };
                                *adj.get_mut(*from).unwrap().get_mut(*to).unwrap() = true;
                                *adj.get_mut(*to).unwrap().get_mut(*from).unwrap() = true;
                            },
                            _ => bail!("`channels` field elements must be tabels"),
                        };
                        Ok(())
                    })?
            }
            _ => bail!("`channels` field must have array type"),
        };
        Ok(Self { nodes, adj })
    }
}

/// Rip-protocol related objects
mod rip {
    use std::{collections::HashMap, time::Duration};
    use super::Graph as RawGRaph;

    use rand::{RngExt, rngs::ThreadRng};

    #[derive(Clone)]
    struct DistanceVectorEntry {
        next: usize,
        hops: usize,
    }

    impl DistanceVectorEntry {
        const INFINITE_HOPS: usize = 16;
        const NONE_NODE: usize = usize::MAX;

        fn initial((node, is_adjacent): (usize, bool)) -> Self {
            if is_adjacent {
                Self { next: node, hops: 1 }
            } else {
                Self { next: Self::NONE_NODE, hops: Self::INFINITE_HOPS }
            }
        }
    }

    #[derive(Clone)]
    struct DistanceVector(Vec<DistanceVectorEntry>);

    impl DistanceVector {
        fn from_adjacence_row(row: Vec<bool>) -> Self {
            Self {
                0: row.into_iter().enumerate().map(DistanceVectorEntry::initial).collect(),
            }
        }
    }

    struct Node {
        dv: DistanceVector,
        has_updates: bool,
    }

    impl Node {
        fn from_adjacence_row(row: Vec<bool>) -> Self {
            Self {
                dv: DistanceVector::from_adjacence_row(row),
                has_updates: true,
            }
        }

        fn accept_distance_vector(&mut self, neighbor: usize, dv: &DistanceVector) -> bool {
            self.dv.0.iter_mut().zip(dv.0.iter()).for_each(|(mine, other)| {
                if other.hops != DistanceVectorEntry::INFINITE_HOPS && mine.hops > other.hops + 1 {
                    mine.hops = other.hops + 1;
                    mine.next = neighbor;
                    self.has_updates = true;
                }
            });
            self.has_updates
        }

        fn take_distance_vector(&mut self) -> DistanceVector {
            self.has_updates = false;
            self.dv.clone()
        }
    }

    struct UpdateQueue {
        queue: Vec<usize>,
        rng: ThreadRng,
    }

    impl UpdateQueue {
        fn new_full(nodes: usize) -> Self {
            Self {
                queue: Vec::from_iter(0..nodes),
                rng: rand::rng(),
            }
        }

        fn add(&mut self, node: usize) {
            if !self.queue.contains(&node) {
                self.queue.push(node);
            }
        }

        fn take(&mut self) -> Option<usize> {
            if self.queue.is_empty() {
                return None;
            }
            let elem = self.rng.random_range(0..self.queue.len());
            Some(self.queue.swap_remove(elem))
        }
    }

    pub struct Graph {
        update_queue: UpdateQueue,
        neighbors: Vec<Vec<usize>>,
        nodes: Vec<Node>,
        names: HashMap<usize, String>,
    }

    impl Graph {
        pub fn from_graph(graph: RawGRaph) -> Self {
            Self {
                update_queue: UpdateQueue::new_full(graph.adj.len()),
                neighbors: graph
                    .adj
                    .iter()
                    .map(|row| row
                        .iter()
                        .enumerate()
                        .filter_map(|(ind, adj)| {
                            if *adj {
                                Some(ind)
                            } else {
                                None
                            }
                        })
                        .collect()
                    )
                    .collect(),
                nodes: graph.adj.into_iter().map(Node::from_adjacence_row).collect(),
                names: graph.nodes.into_iter().map(|(name, ind)| (ind, name)).collect(),
            }
        }

        pub fn run(&mut self, show_intermediate: bool) {
            let mut steps = 0;
            while let Some(notifier) = self.update_queue.take() {
                std::thread::sleep(Duration::from_millis(300));
                println!("{} sends updates", self.names.get(&notifier).unwrap());
                let dv = self.nodes.get_mut(notifier).unwrap().take_distance_vector();
                self.neighbors.get(notifier).unwrap().iter().for_each(|nei| {
                    if self.nodes.get_mut(*nei).unwrap().accept_distance_vector(notifier, &dv) {
                        self.update_queue.add(*nei);
                    }
                });
                if show_intermediate {
                    println!("Current configuration:");
                    self.print();
                    println!();
                }
                steps += 1;
            }
            println!("Network stabilized in {steps} steps");
            println!("Final configuration:");
            self.print();
        }

        fn print(&self) {
            (0..self.nodes.len()).for_each(|src| {
                self.print_node(src);
            });
        }

        fn print_node(&self, ind: usize) {
            let node = self.nodes.get(ind).unwrap();
            println!("host {}:", self.names.get(&ind).unwrap());
            println!("    target       |    next hop     |  total hops  ");
            node.dv.0.iter().enumerate().for_each(|(trg, dve)| {
                if trg != ind {
                    if dve.next == DistanceVectorEntry::NONE_NODE {
                        println!(" {:15} |   UNREACHABLE   |    INFINITY    ", self.names.get(&trg).unwrap());
                    } else {
                        println!(" {:15} | {:15} | {}", self.names.get(&trg).unwrap(), self.names.get(&dve.next).unwrap(), dve.hops);
                    }
                }
            });
            println!();
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let graph = Graph::from_toml(
        std::fs::read_to_string(args.config)
            .with_context(|| "Config file must be in toml format")?
            .parse::<Table>()
            .with_context(|| "Config file must be a valid toml")?)?;
    rip::Graph::from_graph(graph).run(args.show_intermediate);
    Ok(())
}
