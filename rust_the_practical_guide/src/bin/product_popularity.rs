use std::collections::HashMap;

fn main() {
    let mut products = HashMap::new();
    products.insert("Product 1", vec![1, 2, 2, 3]);
    products.insert("Product 2", vec![4, 5, 6, 3, 4]);
    products.insert("Product 3", vec![8, 8, 7, 6, 5, 4, 4, 1]);

    for (id, popularity_scores) in products {
        if popularity_analysis(&popularity_scores) {
            println!("{} popularity is increasing or dec", id);
        } else {
            println!("{} popularity is fluctuating", id);
        }
    }
}

fn popularity_analysis(scores: &[i32]) -> bool {
    let inc = scores.windows(2).all(|w| w[0] <= w[1]);
    let dec = scores.windows(2).all(|w| w[0] >= w[1]);
    inc || dec
}
