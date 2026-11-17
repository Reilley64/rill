//! Iterative SCC classification and cause propagation over alias obligations.
use super::*;

pub(super) fn alias_causes(
    imports: &[Import],
    analyses: &[Vec<(String, Span, Resolution)>],
    host: &mut Host,
) -> Result<Vec<u8>> {
    let resolutions: Vec<_> = analyses.iter().flatten().map(|(_, _, r)| r).collect();
    host.charge(resolutions.len())?;
    let mut producers: BTreeMap<(ModuleKey, String), Vec<usize>> = BTreeMap::new();
    let mut at = 0;
    for (import, results) in imports.iter().zip(analyses) {
        for (name, _, _) in results {
            if import.public {
                producers
                    .entry((import.module.clone(), name.clone()))
                    .or_default()
                    .push(at);
            }
            at += 1;
        }
    }
    let mut edges = vec![Vec::new(); resolutions.len()];
    let mut reverse = edges.clone();
    for (id, r) in resolutions.iter().enumerate() {
        if !r.empty() {
            continue;
        }
        for dependency in &r.dependencies {
            for &other in producers.get(dependency).into_iter().flatten() {
                host.charge(1)?;
                if resolutions[other].empty() {
                    edges[id].push(other);
                    reverse[other].push(id);
                }
            }
        }
        edges[id].sort();
        edges[id].dedup();
    }
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::new();
    for root in 0..edges.len() {
        if visited[root] {
            continue;
        }
        let mut stack = vec![(root, 0)];
        visited[root] = true;
        while let Some((node, next)) = stack.last_mut() {
            host.charge(1)?;
            if let Some(&child) = edges[*node].get(*next) {
                *next += 1;
                if !visited[child] {
                    visited[child] = true;
                    stack.push((child, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut component = vec![usize::MAX; edges.len()];
    let mut components = Vec::new();
    for &root in order.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        let id = components.len();
        let mut members = Vec::new();
        let mut stack = vec![root];
        component[root] = id;
        while let Some(node) = stack.pop() {
            host.charge(1)?;
            members.push(node);
            host.charge(reverse[node].len())?;
            for &parent in &reverse[node] {
                if component[parent] == usize::MAX {
                    component[parent] = id;
                    stack.push(parent);
                }
            }
        }
        components.push(members);
    }
    let mut causes = vec![0; components.len()];
    let mut dependencies = vec![BTreeSet::new(); components.len()];
    let mut dependents = vec![BTreeSet::new(); components.len()];
    for (id, members) in components.iter().enumerate() {
        for &node in members {
            host.charge(1)?;
            if !resolutions[node].empty() {
                continue;
            }
            if resolutions[node].private {
                causes[id] |= 2;
            }
            if resolutions[node].missing {
                causes[id] |= 1;
            }
            if members.len() > 1 || edges[node].contains(&node) {
                causes[id] |= 4;
            }
            host.charge(edges[node].len())?;
            for &other in &edges[node] {
                let dependency = component[other];
                if dependency != id {
                    dependencies[id].insert(dependency);
                    dependents[dependency].insert(id);
                }
            }
        }
    }
    let mut remaining: Vec<_> = dependencies.iter().map(BTreeSet::len).collect();
    let mut ready: VecDeque<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(id, &n)| (n == 0).then_some(id))
        .collect();
    while let Some(id) = ready.pop_front() {
        host.charge(1)?;
        if causes[id] == 0 && components[id].iter().any(|&node| resolutions[node].empty()) {
            causes[id] = 1;
        }
        for &parent in &dependents[id] {
            host.charge(1)?;
            causes[parent] |= causes[id];
            remaining[parent] -= 1;
            if remaining[parent] == 0 {
                ready.push_back(parent);
            }
        }
    }
    Ok(component.iter().map(|&id| causes[id]).collect())
}
