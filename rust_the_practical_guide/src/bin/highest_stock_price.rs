#[derive(Default)]
struct MaxStack {
    main_stack: Vec<i32>,
    max_stack: Vec<i32>,
}

impl MaxStack {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, value: i32) {
        self.main_stack.push(value);
        let new_max = self.max_stack.last().map_or(value, |&top| top.max(value));
        self.max_stack.push(new_max);
    }

    fn pop(&mut self) {
        self.main_stack.pop();
        self.max_stack.pop();
    }

    fn max_value(&self) -> Option<i32> {
        self.max_stack.last().copied()
    }
}

fn main() {
    let mut stack = MaxStack::new();
    stack.push(55);
    stack.push(80);
    stack.push(120);
    stack.push(99);
    stack.push(22);
    stack.push(140);
    stack.push(145);

    print!("Maximum value of stock: ");
    println!("{:}", stack.max_value().unwrap());
    println!("After going one week back");
    print!("Maximum value of stock: ");
    stack.pop();
    println!("{:}", stack.max_value().unwrap());
}
