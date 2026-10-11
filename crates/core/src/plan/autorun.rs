//! The AutoRun editor (spec §5.4): reads the HKCU AutoRun value the way cmd reads it and edits
//! it where it stands, so other programs' commands keep every byte (spec §7, doctrine guard
//! (ค)). Pure, like the rest of the planner.
use super::PlanError;
use crate::model::Target;
use std::ops::Range;

/// One command in an AutoRun value, as [`scan`] finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    /// The command, without the whitespace around it.
    pub(crate) text: String,
    /// Where `text` is in the value, in bytes.
    span: Range<usize>,
    /// The separator before the command, if any, with the one space or tab right before it.
    before: Option<Range<usize>>,
    /// The separator after the command, if any, with the one space or tab right after it.
    after: Option<Range<usize>>,
    /// The command is combined with others in a way this reading does not take apart: it
    /// holds a pipe or a parenthesis that is not quoted or escaped, or an unclosed quote runs
    /// it into the rest of the value.
    combined: bool,
}

impl Part {
    /// The bytes that go when this command is taken out of the value: the command, one
    /// separator next to it with the whitespace between them, and one space or tab on the far
    /// side of that separator, where our own ` & ` puts one. The separator is the one before
    /// the command, or for the first command the one after it. Every other byte stays as it
    /// was, so taking out an entry that was added with ` & `, after the value or in front of
    /// it, gives back the value as it was before.
    ///
    /// One exception keeps the commands that are left as cmd read them: when taking out the
    /// separator before would put a `|` right before the `|` of a `||` after the command,
    /// cmd would pair the two, so the separator after the command goes instead.
    fn cut(&self, value: &str) -> Range<usize> {
        let bar_at = |i: Option<usize>| i.and_then(|i| value.as_bytes().get(i)) == Some(&b'|');
        match (&self.before, &self.after) {
            (Some(before), Some(after))
                if bar_at(before.start.checked_sub(1)) && bar_at(Some(self.span.end)) =>
            {
                self.span.start..after.end
            }
            (Some(before), _) => before.start..self.span.end,
            (None, Some(after)) => self.span.start..after.end,
            (None, None) => self.span.clone(),
        }
    }
}

/// An AutoRun value as cmd reads it (see [`scan`]).
pub(crate) struct Scan {
    /// The commands, in order.
    pub(crate) parts: Vec<Part>,
    /// The value ends cleanly: not inside quotes, not with a lone `^`, and not with a
    /// separator. Text added after a value that does not would be quoted, escaped, or read
    /// after an empty command.
    pub(crate) clean_end: bool,
}

/// Reads an AutoRun value the way cmd reads it: `"` starts and ends quoted text; outside
/// quotes `^` makes the character after it plain text, so `^&`, `^|` and `^"` are text; and
/// outside quotes `&`, `&&` and `||` separate commands, while a single `|` is a pipe inside
/// one command. Each command is trimmed of the whitespace around it (quoted or escaped
/// whitespace is text), and an empty command, as in `a & & b`, is no part.
pub(crate) fn scan(value: &str) -> Scan {
    let mut parts = Vec::new();
    let mut chars = value.char_indices().peekable();
    let mut quoted = false;
    // The command being read: where its text is, the separator before it, and whether it
    // holds a pipe or a parenthesis.
    let mut span: Option<Range<usize>> = None;
    let mut before = None;
    let mut combined = false;
    // Whether the last character read is a space or tab between words: not quoted, not
    // escaped.
    let mut blank = false;
    let mut lone_caret = false;
    while let Some((i, c)) = chars.next() {
        // `&&` or `||`.
        let doubled = chars.peek().is_some_and(|&(_, next)| next == c);
        if !quoted && (c == '&' || (c == '|' && doubled)) {
            let mut end = i + 1;
            if doubled {
                chars.next();
                end += 1;
            }
            let blank_after = matches!(chars.peek(), Some(&(_, ' ' | '\t')));
            // A space or tab is one byte, so it starts one byte before the separator.
            let start = i - usize::from(blank);
            let after = i..end + usize::from(blank_after);
            push_part(
                &mut parts,
                value,
                span.take(),
                before.take(),
                Some(after),
                combined,
            );
            before = Some(start..end);
            combined = false;
            blank = false;
            continue;
        }
        let mut end = i + c.len_utf8();
        let text = match c {
            '"' => {
                quoted = !quoted;
                true
            }
            '^' if !quoted => {
                match chars.next() {
                    Some((j, escaped)) => end = j + escaped.len_utf8(),
                    None => lone_caret = true,
                }
                true
            }
            '|' | '(' | ')' if !quoted => {
                combined = true;
                true
            }
            _ => quoted || !c.is_whitespace(),
        };
        blank = !text && matches!(c, ' ' | '\t');
        if text {
            span = Some(span.map_or(i..end, |s| s.start..end));
        }
    }
    let clean_end = !quoted && !lone_caret && (span.is_some() || before.is_none());
    // An unclosed quote runs the last command into whatever the quote holds.
    push_part(&mut parts, value, span, before, None, combined || quoted);
    Scan { parts, clean_end }
}

/// The commands of an AutoRun value (see [`scan`]).
pub(crate) fn parts(value: &str) -> Vec<Part> {
    scan(value).parts
}

/// Adds the command at `span` to `parts`, when there is one.
fn push_part(
    parts: &mut Vec<Part>,
    value: &str,
    span: Option<Range<usize>>,
    before: Option<Range<usize>>,
    after: Option<Range<usize>>,
    combined: bool,
) {
    if let Some(span) = span {
        // `scan` puts both ends of `span` on character boundaries, so `get` finds the text.
        let text = value.get(span.clone()).unwrap_or_default().to_string();
        parts.push(Part {
            text,
            span,
            before,
            after,
            combined,
        });
    }
}

/// `value` without the bytes in `cut`.
fn without(value: &str, cut: &Range<usize>) -> String {
    value
        .char_indices()
        .filter(|(i, _)| !cut.contains(i))
        .map(|(_, c)| c)
        .collect()
}

/// Takes every command that `pick` chooses out of `value`, each as [`Part::cut`] says, and
/// says whether it took any. One at a time: what is left is read again before the next one
/// goes, so two commands side by side each take their own separator.
fn take_out(value: &mut String, pick: impl Fn(&str) -> bool) -> bool {
    let mut took = false;
    while let Some(part) = parts(value).into_iter().find(|p| pick(&p.text)) {
        *value = without(value, &part.cut(value));
        took = true;
    }
    took
}

/// The HKCU AutoRun value after the plan, and whether another Clink was taken out of it
/// (spec §5.4). The value is edited where it stands, not taken apart and written again, so
/// every byte of other programs' commands stays as it was (spec §7, doctrine guard (ค)):
/// 1. Our entries go, old and current: they are the commands that hold `ours`, our profile
///    path, in any mix of upper and lower case, as Windows reads paths. A command of ours
///    that is combined with another one (a pipe, parentheses or an unclosed quote; see
///    [`Part`]) is not taken apart: the plan fails with `E_AUTORUN_WRITE`, and the user
///    edits AutoRun by hand.
/// 2. Another command that runs Clink fails the plan, unless the user agreed to turn it off
///    (`replace_foreign`); then it goes too. When it is combined with another command, it
///    is not taken apart either: the plan fails with `E_AUTORUN_WRITE`.
/// 3. For `Target::All` our entry is added with ` & `: after the value when the value ends
///    cleanly (see [`Scan::clean_end`]), and in front of it, as `<entry> & <value>`, when it
///    does not. When nothing else is left, our entry is the whole value.
///
/// Limit: text joined to a command that goes with no separator at all, as in
/// `echo ^& <ours>`, goes with it.
pub(crate) fn edit_autorun(
    current: &str,
    target: Target,
    entry: &str,
    ours: &str,
    replace_foreign: bool,
) -> Result<(String, bool), PlanError> {
    let ours = ours.to_lowercase();
    let is_ours = |p: &str| p.to_lowercase().contains(&ours);
    if let Some(part) = parts(current)
        .into_iter()
        .find(|p| p.combined && is_ours(&p.text))
    {
        return Err(PlanError::AutoRunNeedsHandEdit { part: part.text });
    }
    let mut value = current.to_string();
    take_out(&mut value, is_ours);
    let foreign = |p: &str| p.to_ascii_lowercase().contains("clink");
    if !replace_foreign {
        if let Some(existing) = parts(&value).into_iter().find(|p| foreign(&p.text)) {
            return Err(PlanError::ForeignClink {
                existing: existing.text,
            });
        }
    }
    if let Some(part) = parts(&value)
        .into_iter()
        .find(|p| p.combined && foreign(&p.text))
    {
        return Err(PlanError::AutoRunNeedsHandEdit { part: part.text });
    }
    let replaced = take_out(&mut value, foreign);
    if target == Target::All {
        value = if value.trim().is_empty() {
            entry.to_string()
        } else if scan(&value).clean_end {
            format!("{value} & {entry}")
        } else {
            format!("{entry} & {value}")
        };
    }
    Ok((value, replaced))
}
