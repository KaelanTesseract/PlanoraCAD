# Translating PlanoraCAD

PlanoraCAD comes in German and English. You can add any other language yourself, without programming. Each language is one text file.

## Step by step

1. Download the English file `en.ftl` from GitHub: [crates/planora-i18n/languages/en.ftl](https://github.com/KaelanTesseract/PlanoraCAD/blob/master/crates/planora-i18n/languages/en.ftl). Click "Download raw file" (the arrow icon above the text).
2. Rename it after your language code, for example `fr.ftl` (French), `es.ftl` (Spanish) or `pt-BR.ftl` (Portuguese, Brazil). The file name is the language code; `de` and `en` are built in and cannot be replaced.
3. Open the file in a plain text editor (for example Notepad) and save it as UTF-8.
4. Set the two lines at the top:
   ```
   language-name = Français
   decimal-separator = ,
   ```
   - `language-name` is required. It is the name of the language in the language itself, as it should appear in the list. Without it, the file is not loaded.
   - `decimal-separator` is `,` or `.`. Numbers are shown with it (`38,8` or `38.8`). If the line is missing, `.` is used. Any other value makes the file invalid.
5. Translate the text after each `=`. Do not change the part before `=` (the key).
6. Put the file in the folder `%APPDATA%\PlanoraCAD\languages`. To get there, type `%APPDATA%\PlanoraCAD` into the address bar of Windows Explorer and create the folder `languages` if it does not exist. (The folder `languages` next to `PlanoraCAD.exe` works too. If both have a file with the same name, a valid file in `%APPDATA%` wins.)
7. Restart PlanoraCAD. Choose your language under "File → Settings …", section "Appearance", "Language".

You do not have to translate everything at once. Any text that is missing in your file appears in English.

## Rules of the file format

The files use the [Fluent](https://projectfluent.org/) format. You only need a few rules:

- **Placeholders** like `{ $name }` or `{ $count }` are filled in by the program. Keep them exactly as they are; you may move them within the sentence.
- **Plurals** look like this. Keep the structure and translate the words:
  ```
  test-count = { $count ->
      [one] { $count } file
     *[other] { $count } files
  }
  ```
  Your language may have other plural forms (`[few]`, `[many]`); you can add them.
- **Lines that start with spaces** continue the text of the line above. Keep the indentation.
- **Lines that start with `#`** are comments. You do not need to translate them.
- **Each key only once.** If a key appears twice, the whole file is invalid.
- **Numbers inside the texts** (like `0.5 mm`) should use your decimal separator.

## If something goes wrong

A broken file never makes the program crash. If the file has an error (wrong syntax, a key twice, `language-name` missing, wrong `decimal-separator`), PlanoraCAD skips the whole file and starts with another language. The reason is written to the log file `%APPDATA%\PlanoraCAD\planora.log`, in a line that contains `Language file skipped:`. Fix the error and restart.

PlanoraCAD also compares the placeholders (`{ $name }`) of every text with the English file. If a text uses other placeholders than in English (one missing, one misspelled, one extra), the file still loads, but the log gets a line with `Language file problem:` that names the key, for example `"feature-param-cycle" uses the placeholders none but English uses $list`. Such a text may show a gap or a placeholder name instead of the value, so fix it.

If a single text is missing or its key is misspelled, only that text appears in English.

## Limits

- **No right-to-left scripts.** Arabic, Hebrew and other right-to-left languages do not display correctly yet.
- **Only scripts covered by the built-in fonts.** The program uses its own fonts (Montserrat and Roboto). They cover Latin, Greek and Cyrillic letters. Chinese, Japanese, Korean, Thai, Hindi and many other scripts do not work yet.
- **PDF drawings** only support Western European letters (Windows-1252). A drawing exported in another language may lose letters; the export tells you if a text cannot be written.
- **Dates** stay in the form `dd.mm.yyyy`.
- **Units** stay millimeters.
- **Logs and crash reports** are always English, whatever language you choose.

## Sharing your translation

We are happy to include your language in PlanoraCAD.

- **With a GitHub account:** Open a pull request that adds your file to `crates/planora-i18n/languages/`.
- **Without a GitHub account:** Use "Help → Feedback …" in the program, choose "Wish" and click "Send without a GitHub account". The form that opens takes `.txt` and `.zip` files, so rename your file to `fr.ftl.txt` or put it in a ZIP file.

When the program gets new texts, they appear in English in your language until someone translates them. Compare your file with the newest `en.ftl` to find new keys.
