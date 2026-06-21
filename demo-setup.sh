#!/usr/bin/env bash
set -e

rm -rf /tmp/rugit-demo
mkdir /tmp/rugit-demo
cd /tmp/rugit-demo

git init -q
git config user.email "demo@example.com"
git config user.name "Demo User"

cat > main.rs << 'EOF'
fn main() {
    println!("Hello, world!");
}
EOF

cat > lib.rs << 'EOF'
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
EOF

git add .
git commit -q -m "initial commit"

# Add a function to main.rs (unstaged change)
cat >> main.rs << 'EOF'

fn greet(name: &str) {
    println!("Hello, {}!", name);
}
EOF

# Untracked file
cat > TODO.md << 'EOF'
# TODO
- Add tests
- Improve docs
EOF
