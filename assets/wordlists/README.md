# Wordlists

Ranked wordlists for the on-screen keyboard's suggestions: one lowercase word per line, most
frequent first. They are embedded in the binary with `include_str!`.

| File              | Words  | Source                                                                                                 |
| ----------------- | ------ | ------------------------------------------------------------------------------------------------------ |
| `wordlist-en.txt` | 30,000 | [hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) `content/2018/en/en_50k.txt` |
| `wordlist-de.txt` | 28,171 | [hermitdave/FrequencyWords](https://github.com/hermitdave/FrequencyWords) `content/2018/de/de_50k.txt` |

## License

The lists are derived from FrequencyWords, which is built from OpenSubtitles 2018. Its content is
licensed under [CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), and so are these
lists: credit FrequencyWords (Hermit Dave) and keep the share-alike when you change or pass them on.
This license covers only the files in this directory, not the rest of Corona.

## Regenerating

From the 50k file of the language (`word count` per line):

1. Drop words seen fewer than 150 times.
2. Keep only words of letters with at most one inner apostrophe
   (regex `[^\W\d_]+(?:'[^\W\d_]+)?`), lowercase, without duplicates.
3. For English, drop single letters other than `a` and `i`.
4. Keep the first 30,000.

A new language works the same way: add its file here and a `Language` variant in
`crates/osk/src/predict/mod.rs`.
