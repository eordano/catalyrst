pub struct SkinView<'a> {
    pub node_children: &'a [Vec<usize>],

    pub joints: &'a [usize],

    pub skeleton: Option<usize>,
}

pub fn resolve_root_joint(skin: &SkinView<'_>) -> Option<usize> {
    if let Some(sk) = skin.skeleton {
        return Some(sk);
    }
    let parent = parent_indices(skin.node_children);
    lowest_common_ancestor(skin.joints, &parent)
}

fn parent_indices(node_children: &[Vec<usize>]) -> Vec<isize> {
    let mut parent = vec![-1isize; node_children.len()];
    for (i, children) in node_children.iter().enumerate() {
        for &child in children {
            parent[child] = i as isize;
        }
    }
    parent
}

fn lowest_common_ancestor(joints: &[usize], parent: &[isize]) -> Option<usize> {
    let mut chain: Option<Vec<usize>> = None;
    let mut common_ancestor: isize = -1;

    for &node_id in joints {
        if !compare_to(node_id, parent, &mut chain, &mut common_ancestor) {
            return None;
        }
    }

    if common_ancestor >= 0 {
        Some(common_ancestor as usize)
    } else {
        None
    }
}

fn compare_to(
    node_id: usize,
    parent: &[isize],
    chain: &mut Option<Vec<usize>>,
    common_ancestor: &mut isize,
) -> bool {
    let mut node_chain: Vec<usize> = Vec::new();
    let mut curr: isize = node_id as isize;
    while curr >= 0 {
        if curr == *common_ancestor {
            return true;
        }
        node_chain.insert(0, curr as usize);
        curr = parent[curr as usize];
    }

    match chain {
        None => {
            *chain = Some(node_chain);
        }
        Some(chain) => {
            let depth = chain.len().min(node_chain.len());
            for i in 0..depth {
                if chain[i] != node_chain[i] {
                    if i > 0 {
                        chain.truncate(i);
                        break;
                    }
                    return false;
                }
            }
        }
    }

    *common_ancestor = *chain.as_ref().unwrap().last().unwrap() as isize;
    true
}
