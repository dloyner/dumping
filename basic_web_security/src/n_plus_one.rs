//! N+1 query problem (the classic ORM pitfall)
//!
//! Big idea: 1 query for a list of N rows + N queries for each row's relation
//! = N+1 round-trips. The cost is mostly network latency, and it stays invisible
//! with small dev data. Fix: JOIN (1 query) or batch load with `WHERE id IN (...)`
//! (2 queries). In SeaORM: `find_also_related` / `load_one`.
//! Diesel: `inner_join` / `belonging_to` + `grouped_by`.
//!
//! A fake in-memory "database" counts queries so you can SEE the difference.

use std::{cell::Cell, collections::HashMap};

pub struct Post {
    pub title: String,
    pub author_id: u32,
}

pub struct User {
    pub id: u32,
    pub name: String,
}

pub struct FakeDb {
    posts: Vec<Post>,
    users: Vec<User>,
    queries: Cell<usize>,
}

impl FakeDb {
    pub fn seeded(n_posts: u32, n_users: u32) -> Self {
        let users = (1..=n_users)
            .map(|id| User {
                id,
                name: format!("user{id}"),
            })
            .collect();
        let posts = (0..n_posts)
            .map(|i| Post {
                title: format!("post{i}"),
                author_id: i % n_users + 1,
            })
            .collect();
        Self {
            posts,
            users,
            queries: Cell::new(0),
        }
    }

    pub fn query_count(&self) -> usize {
        self.queries.get()
    }

    fn count(&self) {
        self.queries.set(self.queries.get() + 1);
    }

    /// SELECT * FROM posts
    pub fn all_posts(&self) -> &[Post] {
        self.count();
        &self.posts
    }

    /// SELECT * FROM users WHERE id = $1
    pub fn user_by_id(&self, id: u32) -> Option<&User> {
        self.count();
        self.users.iter().find(|u| u.id == id)
    }

    /// SELECT * FROM users WHERE id IN (...)
    pub fn users_by_ids(&self, ids: &[u32]) -> Vec<&User> {
        self.count();
        self.users.iter().filter(|u| ids.contains(&u.id)).collect()
    }
}

/// BAD: one extra query per post.
pub fn titles_with_authors_n_plus_one(db: &FakeDb) -> Vec<(String, String)> {
    db.all_posts()
        .iter()
        .map(|p| {
            let author = db
                .user_by_id(p.author_id)
                .map(|u| u.name.clone())
                .unwrap_or_default();
            (p.title.clone(), author)
        })
        .collect()
}

/// GOOD: collect the ids, load all authors in ONE query, then join in memory.
pub fn titles_with_authors_batched(db: &FakeDb) -> Vec<(String, String)> {
    let posts = db.all_posts();
    let mut ids: Vec<u32> = posts.iter().map(|p| p.author_id).collect();
    ids.sort_unstable();
    ids.dedup();

    let by_id: HashMap<u32, &str> = db
        .users_by_ids(&ids)
        .into_iter()
        .map(|u| (u.id, u.name.as_str()))
        .collect();

    posts
        .iter()
        .map(|p| {
            (
                p.title.clone(),
                by_id
                    .get(&p.author_id)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_result_very_different_query_count() {
        let bad_db = FakeDb::seeded(100, 10);
        let bad = titles_with_authors_n_plus_one(&bad_db);
        assert_eq!(bad_db.query_count(), 101); // 1 + N

        let good_db = FakeDb::seeded(100, 10);
        let good = titles_with_authors_batched(&good_db);
        assert_eq!(good_db.query_count(), 2); // constant

        assert_eq!(bad, good);
    }
}
