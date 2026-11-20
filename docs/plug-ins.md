# PlanoraCAD – Plug-ins

Plug-ins sind kleine Zusatzprogramme, die PlanoraCAD um Befehle erweitern. Sie laufen in einer abgeschirmten Umgebung (Sandbox) und dürfen nur das, was du ihnen erlaubst. Wie du selbst eines schreibst, steht in der [Entwickler-Anleitung](plugins/entwickler.md).

## Inhalt

- [Wo du Plug-ins findest](#wo-du-plug-ins-findest)
- [Plug-in installieren](#plug-in-installieren)
- [Plug-in benutzen](#plug-in-benutzen)
- [Rechte](#rechte)
- [Updates](#updates)
- [Wenn ein Plug-in ausfällt](#wenn-ein-plug-in-ausfällt)
- [Plug-in entfernen](#plug-in-entfernen)

## Wo du Plug-ins findest

Plug-ins müssen unter einer Lizenz stehen, die zur GPL-3.0 passt. In dieser Version liefert der Installer noch keine Plug-ins mit; du bekommst sie als Datei (Endung `.pcplugin`) oder über den GitHub-Link des Entwicklers. Installiere nur Plug-ins, denen du vertraust. Signaturen prüft PlanoraCAD in dieser Version noch nicht.

## Plug-in installieren

1. Öffne **Datei → Einstellungen …** und dort den Bereich **Plug-ins**.
2. Wähle **Plug-in installieren …**.
3. Entweder **Aus Datei …** und die `.pcplugin` auswählen, oder in das Feld **Repo-Link** den GitHub-Link des Plug-ins eintragen (zum Beispiel `github.com/besitzer/mein-plugin`) und **Holen** wählen.
4. Prüfe im Dialog Name, Autor, Lizenz und gewünschte Rechte und bestätige.

Beim Repo-Link nimmt PlanoraCAD die neueste passende Veröffentlichung (Release) des Repos. Dort muss genau eine `.pcplugin` liegen; der Tag der Veröffentlichung ist die Version.

## Plug-in benutzen

Die Befehle eines Plug-ins stehen unter **Datei → Zusatzmodule**. Plug-ins öffnen eigene Dialoge und Meldungen.

## Rechte

Ein neu installiertes Plug-in darf nichts. Braucht es ein Recht, zum Beispiel den Zugriff auf eine Webseite, fragt PlanoraCAD im Moment des ersten Gebrauchs: **Erlauben**, **Nur diesmal** oder **Ablehnen**. Unter **Datei → Einstellungen … → Plug-ins** siehst du bei jedem Plug-in, welche Rechte es hat, und kannst sie mit **Zurücknehmen** wieder entziehen.

## Updates

Für Plug-ins, die du über einen Repo-Link installiert hast, sucht PlanoraCAD beim Start nach neuen Versionen. Gibt es welche, erscheint ein Hinweis mit **Anzeigen**. In den Einstellungen findest du bei dem Plug-in **Nach Updates suchen** und **Aktualisieren auf …**. Deine Rechte bleiben erhalten, solange Autor und die erlaubten Internet-Adressen gleich bleiben; sonst fragt PlanoraCAD neu.

## Wenn ein Plug-in ausfällt

Hängt ein Plug-in, braucht es zu lange oder zu viel Speicher, hält PlanoraCAD es an und zeigt eine Meldung. PlanoraCAD selbst läuft weiter. Mit **Neu starten** startest du das Plug-in wieder. Fällt es mehrmals aus, bleibt es bis zum nächsten Programmstart aus.

## Plug-in entfernen

In den Einstellungen bei dem Plug-in **Entfernen** wählen. Plug-ins im Programmordner (mitgeliefert) kannst du ausschalten, aber nicht entfernen.
