Output demo.gif

Set Shell "zsh"
Set FontSize 14
Set Width 1200
Set Height 700
Set Theme "Dracula"
Set TypingSpeed 80ms

# --- hidden setup: create a demo repo ---
Hide

Type "bash /Users/damien/dev/rugit/demo-setup.sh && cd /tmp/rugit-demo"
Enter
Sleep 1000ms

Show
# clear scrollback before the demo starts
Type "clear"
Enter
Sleep 300ms

Type "rugit"
Enter
Sleep 1500ms

# Navigate to main.rs
# cursor=0(Spacer) → j×1=Untracked hdr → j×2=TODO.md → j×3=Spacer
# → j×4=Unstaged hdr → j×5=main.rs
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 800ms

# Expand diff inline — pause so viewer can read it
Tab
Sleep 2000ms

# Stage the file
Type "s"
Sleep 1200ms

# Open commit editor
Type "c"
Sleep 300ms
Type "c"
Sleep 800ms

# Type commit message and save
Type "add greet function"
Sleep 600ms
Ctrl+c
Sleep 100ms
Ctrl+c
Sleep 1500ms

# Open log buffer to show new commit
Type "l"
Sleep 1500ms

# Quit log, then quit rugit
Type "q"
Sleep 600ms
Type "q"
Sleep 500ms
