//! Cutting unified diffs into patches `git apply` accepts.

use std::collections::HashSet;

/// A diff split into lines, with the index of each `@@` hunk header.
struct Hunks<'a> {
    lines: Vec<&'a str>,
    starts: Vec<usize>,
}

impl<'a> Hunks<'a> {
    /// `None` when the diff has no hunks.
    fn parse(diff: &'a str) -> Option<Self> {
        let lines: Vec<&str> = diff.lines().collect();
        let starts: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("@@"))
            .map(|(i, _)| i)
            .collect();
        (!starts.is_empty()).then_some(Self { lines, starts })
    }

    /// Everything before the first hunk (`diff --git`, `---`, `+++`, …).
    fn file_header(&self) -> &[&'a str] {
        &self.lines[..self.starts[0]]
    }

    /// Hunk `index`, starting with its `@@` line.
    fn hunk(&self, index: usize) -> Option<&[&'a str]> {
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).copied().unwrap_or(self.lines.len());
        Some(&self.lines[start..end])
    }
}

fn push_lines<S: AsRef<str>>(out: &mut String, lines: &[S]) {
    for line in lines {
        out.push_str(line.as_ref());
        out.push('\n');
    }
}

/// Parse `@@ -old_start[,count] +new_start[,count] @@` into (old_start, new_start).
fn parse_hunk_starts(header: &str) -> Option<(u32, u32)> {
    let inner = header.strip_prefix("@@ ")?;
    let (old_part, rest) = inner.split_once(' ')?;
    let (new_part, _) = rest.split_once(' ')?;
    let start = |part: &str, sign: char| -> Option<u32> {
        part.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((start(old_part, '-')?, start(new_part, '+')?))
}

/// Whether a diff body line is an addition or removal.
pub fn is_change(line: &str) -> bool {
    line.starts_with(['+', '-'])
}

/// One hunk of `diff` as a complete patch (file header + hunk).
pub fn hunk_patch(diff: &str, hunk_index: usize) -> Option<String> {
    let hunks = Hunks::parse(diff)?;
    let mut patch = String::new();
    push_lines(&mut patch, hunks.file_header());
    push_lines(&mut patch, hunks.hunk(hunk_index)?);
    Some(patch)
}

/// A patch applying only the selected `+`/`-` lines of one hunk.
/// `selected` holds indices into the hunk body (the lines after `@@`).
///
/// The patch applies to the side that still has the unselected changes'
/// "before" state. Staging (`reverse=false`) applies to the index, which holds
/// the `-` lines but not the `+` lines: unselected `-` become context and
/// unselected `+` are dropped. Unstaging or discarding (`reverse=true`) applies
/// in reverse to a side holding the `+` lines, so the roles swap.
///
/// `None` if no selected index is a change line.
pub fn lines_patch(
    diff: &str,
    hunk_index: usize,
    selected: &HashSet<usize>,
    reverse: bool,
) -> Option<String> {
    let hunks = Hunks::parse(diff)?;
    let (header, body) = hunks.hunk(hunk_index)?.split_first()?;
    let (old_start, new_start) = parse_hunk_starts(header)?;
    let (as_context, dropped) = if reverse { ('+', '-') } else { ('-', '+') };

    let mut new_body: Vec<String> = Vec::with_capacity(body.len());
    let mut has_selected = false;
    for (i, &line) in body.iter().enumerate() {
        if is_change(line) && selected.contains(&i) {
            has_selected = true;
            new_body.push(line.to_string());
        } else if let Some(rest) = line.strip_prefix(as_context) {
            new_body.push(format!(" {rest}"));
        } else if !line.starts_with(dropped) {
            new_body.push(line.to_string());
        }
    }
    if !has_selected {
        return None;
    }

    let count = |side: char| {
        new_body
            .iter()
            .filter(|l| l.starts_with([' ', side]))
            .count()
    };
    let mut patch = String::new();
    push_lines(&mut patch, hunks.file_header());
    patch.push_str(&format!(
        "@@ -{old_start},{} +{new_start},{} @@\n",
        count('-'),
        count('+')
    ));
    push_lines(&mut patch, &new_body);
    Some(patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
diff --git a/f b/f
--- a/f
+++ b/f
@@ -1,3 +1,3 @@
 keep
-old1
+new1
 tail
@@ -10,2 +10,3 @@
 ctx
-gone
+added1
+added2
";

    fn set(ix: &[usize]) -> HashSet<usize> {
        ix.iter().copied().collect()
    }

    #[test]
    fn hunk_patch_keeps_header_and_selected_hunk() {
        assert_eq!(
            hunk_patch(DIFF, 1).unwrap(),
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -10,2 +10,3 @@\n ctx\n-gone\n+added1\n+added2\n"
        );
        assert!(hunk_patch(DIFF, 2).is_none());
        assert!(hunk_patch("no hunks\n", 0).is_none());
    }

    #[test]
    fn staging_one_addition_drops_others_and_keeps_removals_as_context() {
        // Body of hunk 1: 0 " ctx", 1 "-gone", 2 "+added1", 3 "+added2".
        assert_eq!(
            lines_patch(DIFF, 1, &set(&[2]), false).unwrap(),
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -10,2 +10,3 @@\n ctx\n gone\n+added1\n"
        );
    }

    #[test]
    fn unstaging_one_removal_keeps_additions_as_context() {
        assert_eq!(
            lines_patch(DIFF, 1, &set(&[1]), true).unwrap(),
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -10,4 +10,3 @@\n ctx\n-gone\n added1\n added2\n"
        );
    }

    #[test]
    fn selection_without_change_lines_is_none() {
        assert!(lines_patch(DIFF, 0, &set(&[0, 3]), false).is_none());
        assert!(lines_patch(DIFF, 0, &set(&[]), false).is_none());
    }

    #[test]
    fn parses_hunk_starts_with_and_without_counts() {
        assert_eq!(parse_hunk_starts("@@ -10,2 +11,3 @@ fn x()"), Some((10, 11)));
        assert_eq!(parse_hunk_starts("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_starts("not a hunk"), None);
    }
}
