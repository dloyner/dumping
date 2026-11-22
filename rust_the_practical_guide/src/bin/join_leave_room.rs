use std::cmp::Ordering;

const PAGE_SIZE: usize = 10;

struct Node {
    name: String,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

#[derive(Default)]
struct Bst {
    root: Option<Box<Node>>,
}

impl Bst {
    // A participant joins. Return false if name already exist
    fn insert(&mut self, name: String) -> bool {
        let mut cur = &mut self.root;
        while let Some(node) = cur {
            match name.cmp(&node.name) {
                Ordering::Less => cur = &mut node.left,
                Ordering::Greater => cur = &mut node.right,
                Ordering::Equal => return false,
            }
        }
        *cur = Some(Box::new(Node {
            name,
            left: None,
            right: None,
        }));
        true
    }

    // A participant leaves. Return false if the name was not found
    fn remove(&mut self, name: &str) -> bool {
        Self::remove_node(&mut self.root, name)
    }

    fn remove_node(slot: &mut Option<Box<Node>>, name: &str) -> bool {
        let Some(node) = slot else {
            return false;
        };
        match name.cmp(node.name.as_str()) {
            Ordering::Less => Self::remove_node(&mut node.left, name),
            Ordering::Greater => Self::remove_node(&mut node.right, name),
            Ordering::Equal => {
                match (node.left.take(), node.right.take()) {
                    (None, None) => *slot = None,
                    (Some(child), None) | (None, Some(child)) => *slot = Some(child),
                    (Some(l), Some(r)) => {
                        node.left = Some(l);
                        node.right = Some(r);
                        node.name = Self::pop_min(&mut node.right);
                    }
                }
                true
            }
        }
    }

    // Remove the smallest node of a non-empty subtree and return its name
    fn pop_min(slot: &mut Option<Box<Node>>) -> String {
        match slot {
            Some(node) if node.left.is_some() => Self::pop_min(&mut node.left),
            _ => {
                let node = slot.take().expect("subtree is non-empty");
                *slot = node.right;
                node.name
            }
        }
    }

    // return up to `limit` names strictly greater than `after`, in order
    fn page_after(&self, after: Option<&str>, limit: usize) -> Vec<&str> {
        let mut out = Vec::with_capacity(limit);
        Self::collect(&self.root, after, limit, &mut out);
        out
    }

    fn collect<'a>(
        slot: &'a Option<Box<Node>>,
        after: Option<&str>,
        limit: usize,
        out: &mut Vec<&'a str>,
    ) {
        let Some(node) = slot else { return };
        if out.len() >= limit {
            return;
        }
        if after.is_none_or(|a| node.name.as_str() > a) {
            Self::collect(&node.left, after, limit, out);
            if out.len() < limit {
                out.push(&node.name);
            }
        }
        if out.len() < limit {
            Self::collect(&node.right, after, limit, out);
        }
    }
}

#[derive(Default)]
struct Gallery {
    cursor: Option<String>,
}

impl Gallery {
    fn next_page(&mut self, bst: &Bst) -> Vec<String> {
        let page: Vec<String> = bst
            .page_after(self.cursor.as_deref(), PAGE_SIZE)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if let Some(last) = page.last() {
            self.cursor = Some(last.clone());
        }
        page
    }

    fn reset(&mut self) {
        self.cursor = None
    }
}

fn main() {
    let mut bst = Bst::default();
    for i in (0..25).rev() {
        bst.insert(format!("user{i:02}"));
    }

    let mut gallery = Gallery::default();
    println!("{:?}", gallery.next_page(&bst)); // user00..user09

    // Someone leaves and someone joins between two calls: no skip, no duplicate
    bst.remove("user05");
    bst.insert("user10a".to_string());
    println!("{:?}", gallery.next_page(&bst)); // user10, user10a, user11..user18
    println!("{:?}", gallery.next_page(&bst)); // user19..user24
    println!("{:?}", gallery.next_page(&bst)); // []
}
