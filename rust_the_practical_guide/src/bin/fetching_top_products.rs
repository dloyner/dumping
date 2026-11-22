use std::fmt;

type Ptr<T> = Option<Box<Node<T>>>;

#[derive(Debug)]
struct LinkedList<T> {
    head: Ptr<T>,
}

#[derive(Debug)]
struct Node<T> {
    element: T,
    next: Ptr<T>,
}

impl<T> LinkedList<T> {
    fn new() -> LinkedList<T> {
        Self { head: None }
    }

    fn add(&mut self, element: T) {
        let next = self.head.take();
        self.head = Some(Box::new(Node { element, next }));
    }

    fn remove(&mut self) -> Option<T> {
        self.head.take().map(|old_head| {
            self.head = old_head.next;
            old_head.element
        })
    }

    fn peek(&self) -> Option<&T> {
        self.head.as_deref().map(|node| &node.element)
    }
}

impl<T: fmt::Debug> LinkedList<T> {
    fn print(&self) {
        let mut cur = &self.head;
        while let Some(node) = cur {
            print!("{:?} ", node.element);
            cur = &node.next;
        }
        println!();
    }
}

fn sorting_lists(vec_list: &mut [LinkedList<i32>]) -> LinkedList<i32> {
    let mut sorted = LinkedList::new();
    let mut tail = &mut sorted.head;

    while let Some((idx, val)) = vec_list
        .iter()
        .enumerate()
        .filter_map(|(i, list)| list.peek().map(|&v| (i, v)))
        .min_by_key(|&(_, v)| v)
    {
        vec_list[idx].remove();
        let node = tail.insert(Box::new(Node {
            element: val,
            next: None,
        }));
        tail = &mut node.next;
    }

    sorted
}

fn main() {
    let mut a = LinkedList::new();
    for x in [9, 5, 1] {
        a.add(x);
    }
    let mut b = LinkedList::new();
    for x in [8, 6, 2] {
        b.add(x);
    }
    let empty = LinkedList::new();

    let mut lists = vec![a, empty, b];
    sorting_lists(&mut lists).print(); // 1 2 5 6 8 9
}
