//! Topological sort of the device DAG (Kahn's algorithm).

/// Order `node_count` nodes so every dependency comes before what depends on it.
///
/// `deps` is a list of `(from, to)` edges meaning "`from` must be processed before `to`"
/// (signal flows `from → to`). Returns one valid topological order, or `None` if the graph
/// has a cycle — which, because the engine solves connections locally with no feedback
/// paths, is always a wiring mistake for `compile` to reject.
///
/// Parallel edges between the same pair of nodes are fine: each is counted as its own
/// dependency and cancelled once, so the in-degree bookkeeping stays consistent. A self-loop
/// (`from == to`) is a cycle. Compile-time only — allocates, never on the hot path.
pub(super) fn topo_sort(node_count: usize, deps: &[(usize, usize)]) -> Option<Vec<usize>> {
    // Kahn's algorithm: repeatedly emit a node with no remaining unmet dependencies.
    let mut in_degree = vec![0usize; node_count];
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for &(from, to) in deps {
        successors[from].push(to);
        in_degree[to] += 1;
    }

    // Seed with every node nothing points at. Order within `ready` is an implementation
    // detail — any topological order is valid — and is deterministic for a given graph.
    let mut ready: Vec<usize> = (0..node_count).filter(|&n| in_degree[n] == 0).collect();
    let mut order = Vec::with_capacity(node_count);
    while let Some(node) = ready.pop() {
        order.push(node);
        for &next in &successors[node] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                ready.push(next);
            }
        }
    }

    // Every node placed ⇒ acyclic. Fewer ⇒ the leftovers form at least one cycle.
    (order.len() == node_count).then_some(order)
}

/// `true` if `dst` is reachable from `src` by following **one or more** edges in `deps` (the same
/// `(from, to)` form as [`topo_sort`]). Used by `compile` to decide whether an edge `from → to`
/// closes a cycle: it does exactly when `from` is reachable from `to`. A self-loop (`src == dst`
/// with the edge present) counts as reachable. Compile-time only — allocates, never on the hot path.
pub(super) fn reaches(node_count: usize, deps: &[(usize, usize)], src: usize, dst: usize) -> bool {
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for &(from, to) in deps {
        successors[from].push(to);
    }
    // Start one step out from `src` so the empty path never counts — reachability requires ≥1 edge.
    let mut seen = vec![false; node_count];
    let mut stack = successors[src].clone();
    while let Some(n) = stack.pop() {
        if n == dst {
            return true;
        }
        if !seen[n] {
            seen[n] = true;
            stack.extend(successors[n].iter().copied());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `true` if `order` is a permutation of `0..n` that respects every `(from, to)` dep.
    fn is_valid_topo(order: &[usize], n: usize, deps: &[(usize, usize)]) -> bool {
        if order.len() != n {
            return false;
        }
        let mut position = vec![usize::MAX; n];
        for (pos, &node) in order.iter().enumerate() {
            if node >= n || position[node] != usize::MAX {
                return false; // out of range or duplicate
            }
            position[node] = pos;
        }
        deps.iter().all(|&(from, to)| position[from] < position[to])
    }

    #[test]
    fn linear_chain_orders_source_to_sink() {
        // 0 → 1 → 2 must come out exactly [0, 1, 2].
        let order = topo_sort(3, &[(0, 1), (1, 2)]).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn diamond_respects_all_dependencies() {
        // 0 → {1, 2} → 3 (fan-out then fan-in). Several valid orders; assert validity.
        let deps = [(0, 1), (0, 2), (1, 3), (2, 3)];
        let order = topo_sort(4, &deps).unwrap();
        assert!(is_valid_topo(&order, 4, &deps), "got {order:?}");
    }

    #[test]
    fn isolated_nodes_are_all_included() {
        // No edges: every node is independent, all must still appear exactly once.
        let order = topo_sort(3, &[]).unwrap();
        assert!(is_valid_topo(&order, 3, &[]), "got {order:?}");
    }

    #[test]
    fn parallel_edges_between_two_nodes_are_fine() {
        // Two connections 0 → 1 (e.g. two ports) — still acyclic, 0 before 1.
        let order = topo_sort(2, &[(0, 1), (0, 1)]).unwrap();
        assert_eq!(order, vec![0, 1]);
    }

    #[test]
    fn a_cycle_has_no_ordering() {
        // 0 → 1 → 2 → 0 is a loop.
        assert!(topo_sort(3, &[(0, 1), (1, 2), (2, 0)]).is_none());
    }

    #[test]
    fn a_self_loop_is_a_cycle() {
        assert!(topo_sort(1, &[(0, 0)]).is_none());
    }

    #[test]
    fn reaches_follows_a_path_but_needs_at_least_one_edge() {
        // 0 → 1 → 2: 0 reaches 1 and 2; 2 reaches neither; a node doesn't reach itself with no loop.
        let deps = [(0, 1), (1, 2)];
        assert!(reaches(3, &deps, 0, 1));
        assert!(reaches(3, &deps, 0, 2));
        assert!(!reaches(3, &deps, 2, 0));
        assert!(
            !reaches(3, &deps, 0, 0),
            "no path back to self ⇒ not reachable"
        );
    }

    #[test]
    fn reaches_detects_the_back_edge_of_a_cycle() {
        // Edge a→b is (0,1); the back edge b→a is (1,0). With both present, b reaches a — so the
        // edge a→b closes a cycle, which is exactly the test `compile` makes. A self-loop counts too.
        assert!(reaches(2, &[(0, 1), (1, 0)], 1, 0));
        assert!(reaches(1, &[(0, 0)], 0, 0), "self-loop reaches itself");
    }
}
