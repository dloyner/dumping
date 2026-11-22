struct Bst {
    root: Node,
}

#[derive(Clone)]
struct Node {
    val: i32,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

impl Node {
    fn new(val: i32) -> Self {
        Self {
            val,
            left: None,
            right: None,
        }
    }

    fn insert(&mut self, new_val: i32) {
        let child = if new_val > self.val {
            &mut self.right
        } else {
            &mut self.left
        };
        match child {
            Some(node) => node.insert(new_val),
            None => *child = Some(Box::new(Node::new(new_val))),
        }
    }
}

fn products_in_range(root: &Node, lo: i32, hi: i32) -> Vec<i32> {
    let mut res = Vec::new();
    traverse(Some(root), lo, hi, &mut res);
    res
}

fn traverse(node: Option<&Node>, lo: i32, hi: i32, res: &mut Vec<i32>) {
    let Some(node) = node else {
        return;
    };

    if node.val >= lo {
        traverse(node.left.as_deref(), lo, hi, res);
    }
    if (lo..=hi).contains(&node.val) {
        res.push(node.val);
    }
    if node.val <= hi {
        traverse(node.right.as_deref(), lo, hi, res);
    }
}

fn main() {
    let product_prices = vec![9, 6, 14, 20, 1, 30, 8, 17, 5];
    let mut bst = Bst {
        root: Node::new(product_prices[0]),
    };

    for &p in product_prices.iter().skip(1) {
        bst.root.insert(p);
    }

    let res = products_in_range(&bst.root, 6, 15);
    println!("{:?}", res);
}
