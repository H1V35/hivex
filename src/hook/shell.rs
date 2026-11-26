//! What a shell command line runs, read from the words in command position of each of its simple
//! commands, so text in arguments or in a here-document, such as a search for `git commit`, is
//! never taken for a call.

use std::str::Chars;

/// Words that run the command after them, such as a command proxy (`rtk`, also `rtk proxy`).
const WRAPPERS: &[&str] = &["env", "command", "exec", "time", "nohup", "sudo", "rtk"];

/// Whether `line` runs `hivex search`, `hivex read` or `hivex relations`.
pub fn recovers(line: &str, escapes: bool) -> bool {
  programs(line, escapes).any(|(name, arguments)| {
    name == "hivex"
      && arguments
        .first()
        .is_some_and(|command| ["search", "read", "relations"].contains(&command.as_str()))
  })
}

/// Whether `line` runs `git commit` or `gh pr create`.
pub fn publishes(line: &str, escapes: bool) -> bool {
  programs(line, escapes).any(|(name, arguments)| match name.as_str() {
    "git" => git_subcommand(&arguments) == Some("commit"),
    "gh" => {
      let mut words = arguments.iter().filter(|word| !word.starts_with('-'));
      words.next().is_some_and(|word| word == "pr")
        && words.next().is_some_and(|word| word == "create")
    }
    _ => false,
  })
}

/// The first word after Git's global options; `-C` and `-c` take a value.
fn git_subcommand(arguments: &[String]) -> Option<&str> {
  let mut words = arguments.iter();
  while let Some(word) = words.next() {
    match word.as_str() {
      "-C" | "-c" => {
        words.next();
      }
      option if option.starts_with('-') => {}
      _ => return Some(word),
    }
  }
  None
}

/// Each simple command's program, without its directory or `.exe`, and its arguments.
fn programs(line: &str, escapes: bool) -> impl Iterator<Item = (String, Vec<String>)> {
  Words::split(line, escapes).into_iter().filter_map(|words| {
    let start = (0..words.len()).find(|&index| {
      let word = &words[index];
      let proxied = word == "proxy" && index > 0 && words[index - 1] == "rtk";
      !assignment(word) && !WRAPPERS.contains(&word.as_str()) && !proxied
    })?;
    let path = &words[start];
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let name = file
      .strip_suffix(".exe")
      .or_else(|| file.strip_suffix(".EXE"))
      .unwrap_or(file);
    Some((name.to_owned(), words[start + 1..].to_vec()))
  })
}

fn assignment(word: &str) -> bool {
  word.split_once('=').is_some_and(|(name, _)| {
    !name.is_empty() && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
  })
}

/// The rest of the current line, without its line break; `None` at the end of the text.
fn next_line(rest: &mut Chars) -> Option<String> {
  let text = rest.as_str();
  if text.is_empty() {
    return None;
  }
  let (line, remaining) = text.split_once('\n').unwrap_or((text, ""));
  *rest = remaining.chars();
  Some(line.to_owned())
}

/// A here-document's delimiter word after quote removal: quoted parts are taken literally and a
/// backslash keeps the next character.
fn delimiter(rest: &mut Chars) -> String {
  let mut word = String::new();
  let mut quote = None;
  while let Some(character) = rest.clone().next() {
    match (quote, character) {
      (None, c) if c.is_whitespace() || ";&|()<>".contains(c) => break,
      (None, '\'' | '"') => quote = Some(character),
      (Some(open), c) if c == open => quote = None,
      (None, '\\') => {
        rest.next();
        word.extend(rest.clone().next());
      }
      _ => word.push(character),
    }
    rest.next();
  }
  word
}

/// The words of each simple command in a line, split at `;`, `&`, `|`, parentheses and line breaks
/// outside quotes, leaving out here-document bodies. Backslash escapes the next character in Bash;
/// PowerShell paths keep theirs.
struct Words {
  commands: Vec<Vec<String>>,
  word: Option<String>,
  quote: Option<char>,
  escapes: bool,
  /// Delimiters of the here-documents whose bodies start on the next line, and whether `<<-` lets
  /// their lines start with tabs.
  documents: Vec<(String, bool)>,
}

impl Words {
  fn split(line: &str, escapes: bool) -> Vec<Vec<String>> {
    let mut words = Self {
      commands: vec![Vec::new()],
      word: None,
      quote: None,
      escapes,
      documents: Vec::new(),
    };
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
      match words.quote {
        Some(quote) => words.quoted(quote, character, &mut characters),
        None => words.unquoted(character, &mut characters),
      }
    }
    words.end_word();
    words.commands
  }

  fn quoted(&mut self, quote: char, character: char, rest: &mut Chars) {
    match character {
      _ if character == quote => self.quote = None,
      '\\' if quote == '"' && self.escapes => self.push(rest.next()),
      _ => self.push(Some(character)),
    }
  }

  fn unquoted(&mut self, character: char, rest: &mut Chars) {
    match character {
      '\'' | '"' => {
        self.quote = Some(character);
        self.word.get_or_insert_with(String::new);
      }
      '\\' if self.escapes => self.push(rest.next()),
      '<' if rest.as_str().starts_with('<') => self.here_document(rest),
      '\n' => {
        self.separate();
        self.skip_documents(rest);
      }
      ';' | '&' | '|' | '(' | ')' | '\r' => self.separate(),
      _ if character.is_whitespace() => self.end_word(),
      _ => self.push(Some(character)),
    }
  }

  /// Reads `<<` or `<<-` and its delimiter; `<<<` is a here-string, whose word stays an argument.
  fn here_document(&mut self, rest: &mut Chars) {
    self.end_word();
    rest.next();
    if rest.as_str().starts_with('<') {
      rest.next();
      return;
    }
    let tabs = rest.as_str().starts_with('-');
    *rest = rest.as_str()[usize::from(tabs)..]
      .trim_start_matches([' ', '\t'])
      .chars();
    let delimiter = delimiter(rest);
    self.documents.push((delimiter, tabs));
  }

  /// Skips the bodies of the here-documents opened on the line just ended.
  fn skip_documents(&mut self, rest: &mut Chars) {
    for (delimiter, tabs) in std::mem::take(&mut self.documents) {
      while let Some(line) = next_line(rest) {
        let line = if tabs {
          line.trim_start_matches('\t')
        } else {
          &line
        };
        if line.trim_end_matches('\r') == delimiter {
          break;
        }
      }
    }
  }

  fn separate(&mut self) {
    self.end_word();
    self.commands.push(Vec::new());
  }

  fn push(&mut self, character: Option<char>) {
    if let Some(character) = character {
      self.word.get_or_insert_with(String::new).push(character);
    }
  }

  fn end_word(&mut self) {
    if let (Some(word), Some(command)) = (self.word.take(), self.commands.last_mut()) {
      command.push(word);
    }
  }
}
