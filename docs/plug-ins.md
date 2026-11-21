# PlanoraCAD – Plug-ins

Plug-ins sind kleine Zusatzprogramme, die PlanoraCAD um Befehle erweitern. Sie laufen in einer abgeschirmten Umgebung (Sandbox) und dürfen nur das, was du ihnen erlaubst. Wie du selbst eines schreibst, steht in der [Entwickler-Anleitung](plugins/entwickler.md).

## Inhalt

- [Wo du Plug-ins findest](#wo-du-plug-ins-findest)
- [Der Katalog](#der-katalog)
- [Plug-in installieren](#plug-in-installieren)
- [Plug-in benutzen](#plug-in-benutzen)
- [Rechte](#rechte)
- [Updates](#updates)
- [Wenn ein Plug-in ausfällt](#wenn-ein-plug-in-ausfällt)
- [Plug-in entfernen](#plug-in-entfernen)

## Wo du Plug-ins findest

Plug-ins müssen unter einer Lizenz stehen, die zur GPL-3.0 passt. Der Installer liefert keine Plug-ins mit. Du findest sie im **Katalog** (Reiter im Fenster **Zusatzmodule**, siehe unten), als Datei (Endung `.pcplugin`) oder über den GitHub-Link des Entwicklers. Installiere nur Plug-ins, denen du vertraust. Signaturen prüft PlanoraCAD in dieser Version noch nicht.

## Der Katalog

Der Katalog ist eine kuratierte Liste von Plug-ins, die zu PlanoraCAD gehören. Beim ersten Start nach der Aktualisierung fragt PlanoraCAD einmal: „Es gibt Zusatzmodule für PlanoraCAD, z. B. Elektronik. Jetzt ansehen?“ Mit **Katalog öffnen** springst du direkt in den Reiter, mit **Später** (oder Esc) schließt du die Frage; sie kommt danach nicht wieder. Den Katalog erreichst du jederzeit über **Datei → Zusatzmodule → Verwalten …**, Reiter **Katalog**.

- Links stehen Symbol, Name und Kategorie; oben kannst du suchen (Name und Beschreibung) und nach Kategorie filtern. Rechts stehen Beschreibung, neueste Version, verlangte Rechte und **Installieren**.
- **Installieren** läuft wie über den Repo-Link: Du siehst im Dialog Name, Autor, Lizenz und Rechte und bestätigst. Das Paket wird von GitHub geholt und die Prüfsumme geprüft. Im Reiter **Installiert** steht als Herkunft „aus dem Katalog“; Updates kommen wie bei jedem Repo-Link-Plug-in.
- PlanoraCAD lädt den Katalog beim Öffnen des Reiters (höchstens alle 10 Minuten neu) und bei der täglichen Update-Suche aus dem öffentlichen GitHub-Repo `KaelanTesseract/PlanoraCAD-Plugins`. Die letzte Fassung bleibt auf der Festplatte; ohne Internet siehst du sie mit dem Hinweis „offline – Stand vom …“. Wurde der Katalog noch nie geladen, steht dort „Der Katalog konnte nicht geladen werden“ mit **Erneut versuchen**.
- Der Katalog zeigt nur Plug-ins von Dennis Guse. Einen Katalogeintrag für dein eigenes Plug-in gibt es nicht auf Zuruf; Plug-ins von anderen installierst du weiter über Datei oder Repo-Link.
- Es gibt keine Bewertungen, Download-Zahlen oder Bildschirmfotos. Eine Signaturprüfung gibt es nicht; geschützt wird nur über HTTPS zu GitHub und die Prüfsumme des Pakets.

## Das Fenster „Zusatzmodule“

Alle Plug-ins verwaltest du im Fenster **Zusatzmodule**: **Datei → Zusatzmodule → Verwalten …** (wie „Skripte und Zusatzmodule“ in Fusion). Es hat zwei Reiter: **Installiert** und **Katalog**. Im Reiter „Installiert“ steht links steht die Liste der Plug-ins mit Haken (ein- oder ausschalten, wirkt sofort und wird gleich gespeichert) und Zustand, rechts alles zum gewählten Plug-in: Version, Herkunft, Autor, Lizenz, Beschreibung, seine Rechte mit ihrem Stand und **Zurücknehmen**, bei einem Repo-Link die Quelle und die Updates, und **Entfernen**. Ein ausgeschaltetes Plug-in bleibt in der Liste, auch nach einem Neustart. Darüber steht, in welchem Ordner die installierten Plug-ins liegen. Wie andere Fenster lässt sich auch dieses abdocken.

## Plug-in installieren

1. Öffne **Datei → Zusatzmodule → Verwalten …**.
2. Entweder **Aus Datei …** wählen und die `.pcplugin` auswählen, oder in das Feld **Repo-Link** den GitHub-Link des Plug-ins eintragen (zum Beispiel `github.com/besitzer/mein-plugin`) und **Holen** wählen.
3. Prüfe im Dialog Name, Autor, Lizenz und gewünschte Rechte und bestätige.

Beim Repo-Link nimmt PlanoraCAD die neueste passende Veröffentlichung (Release) des Repos. Dort muss genau eine `.pcplugin` liegen; der Tag der Veröffentlichung ist die Version.

## Plug-in benutzen

Die Befehle eines Plug-ins stehen unter **Datei → Zusatzmodule**. Plug-ins öffnen eigene Dialoge und Meldungen.

## Rechte

Ein neu installiertes Plug-in darf nichts. Braucht es ein Recht, zum Beispiel den Zugriff auf eine Webseite, fragt PlanoraCAD im Moment des ersten Gebrauchs: **Erlauben** (gilt auf Dauer), **Nur diesmal** (gilt nur für diese eine Anfrage; beim nächsten Mal fragt PlanoraCAD wieder) oder **Ablehnen** (gilt auf Dauer). Die Rechte „Ordner“, „Modell lesen“ und „Modell ändern“ gibt es in dieser Version noch nicht; der Installationsdialog zeigt sie als „in dieser Version noch nicht verfügbar“, und ein Plug-in bekommt sie nicht. Im Fenster **Zusatzmodule** siehst du rechts beim gewählten Plug-in, welche Rechte es hat und wie du entschieden hast, und kannst sie mit **Zurücknehmen** wieder entziehen.

## Updates

Für Plug-ins, die du über einen Repo-Link installiert hast, sucht PlanoraCAD beim Start (höchstens einmal am Tag) nach neuen Versionen; dabei holt es nur die Liste der Veröffentlichungen. Gibt es welche, erscheint ein Hinweis mit **Anzeigen**; er öffnet das Fenster **Zusatzmodule**. Dort findest du beim gewählten Plug-in **Nach Updates suchen** und **Aktualisieren auf …**. Erst wenn du **Aktualisieren auf …** wählst, lädt PlanoraCAD das neue Paket („Wird geholt …“) und zeigt dann die Änderungen und neu verlangte Rechte. Deine Rechte bleiben erhalten, solange Autor, Herkunft (derselbe Repo-Link) und die erlaubten Internet-Adressen gleich bleiben; sonst fragt PlanoraCAD neu. Eine ältere oder gleiche Version spielt PlanoraCAD nie als Update ein.

## Wenn ein Plug-in ausfällt

Hängt ein Plug-in, braucht es zu lange oder zu viel Speicher oder hält es die Oberfläche mehrere Bilder hintereinander mit zu viel Arbeit auf, hält PlanoraCAD es an und zeigt eine Meldung. PlanoraCAD selbst läuft weiter. Mit **Neu starten** startest du das Plug-in wieder. Fällt es mehrmals aus, bleibt es bis zum nächsten Programmstart aus.

## Plug-in entfernen

Im Fenster **Zusatzmodule** das Plug-in wählen und **Entfernen** drücken. Plug-ins im Programmordner (mitgeliefert) kannst du ausschalten, aber nicht entfernen.
