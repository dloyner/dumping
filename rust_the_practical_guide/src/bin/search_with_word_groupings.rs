use std::collections::HashMap;

fn main() {
    let words = vec![
        "The".to_string(),
        "teh".to_string(),
        "het".to_string(),
        "stupid".to_string(),
        "studpi".to_string(),
        "apple".to_string(),
        "appel".to_string(),
    ];

    let mut map: HashMap<Vec<u8>, Vec<&str>> = HashMap::new();

    for w in &words {
        map.entry(anagram(w)).or_default().push(w);
    }

    let word_groups: Vec<Vec<String>> = map
        .values()
        .map(|list| list.iter().map(|w| w.to_string()).collect())
        .collect();

    println!("{word_groups:?}");
}

fn anagram(word: &str) -> Vec<u8> {
    let mut b: Vec<_> = word.bytes().map(|b| b.to_ascii_lowercase()).collect();
    b.sort_unstable();
    b
}
