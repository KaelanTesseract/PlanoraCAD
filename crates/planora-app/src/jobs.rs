//! Arbeitsthreads für lange Rechnungen (Vernetzen, Lesen, Schreiben; Plan P15) mit Fortschritt
//! und Abbrechen. Die Oberfläche fragt jedes Bild nach dem Ergebnis.

use planora_exchange::Progress;
use std::sync::{Arc, mpsc};

/// Zustand eines Auftrags beim Nachfragen.
#[derive(Debug, PartialEq)]
pub enum Poll<T> {
    Pending,
    Ready(T),
    /// Der Thread endete ohne Ergebnis (Panik oder ließ sich nicht starten).
    Lost,
}

/// Ein laufender Auftrag. Fallenlassen bricht ihn ab (der Thread endet beim nächsten Prüfen).
pub struct Job<T> {
    rx: mpsc::Receiver<T>,
    progress: Arc<Progress>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(work: impl FnOnce(&Progress) -> T + Send + 'static) -> Job<T> {
        Job::spawn_with(Arc::new(Progress::new()), work)
    }

    /// Wie [`Job::spawn`] mit vorgegebenem Fortschritt (etwa schon abgebrochen, für Tests).
    pub fn spawn_with(
        progress: Arc<Progress>,
        work: impl FnOnce(&Progress) -> T + Send + 'static,
    ) -> Job<T> {
        Job::spawn_notify(progress, || {}, work)
    }

    /// Wie [`Job::spawn_with`]; `done` läuft im Arbeitsthread, nachdem das Ergebnis abholbereit
    /// ist (etwa um ein Neuzeichnen anzufordern). `work` läuft in der Sprache des startenden
    /// Threads ([`planora_i18n::current`] beim Start), damit Meldungen aus dem Arbeitsthread in
    /// der Sprache des Fensters erscheinen.
    pub fn spawn_notify(
        progress: Arc<Progress>,
        done: impl FnOnce() + Send + 'static,
        work: impl FnOnce(&Progress) -> T + Send + 'static,
    ) -> Job<T> {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&progress);
        let lang = planora_i18n::current();
        // Lässt sich kein Thread starten, ist der Sender schon fort: `poll` meldet `Lost`.
        let _ = std::thread::Builder::new()
            .name("planora-austausch".into())
            .spawn(move || {
                let result = planora_i18n::with_lang(&lang, || work(&shared));
                let _ = tx.send(result);
                done();
            });
        Job { rx, progress }
    }
}

impl<T> Job<T> {
    /// Fragt nach dem Ergebnis. Nach `Ready` meldet jede weitere Abfrage `Lost`: den Auftrag
    /// danach verwerfen.
    pub fn poll(&self) -> Poll<T> {
        match self.rx.try_recv() {
            Ok(v) => Poll::Ready(v),
            Err(mpsc::TryRecvError::Empty) => Poll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => Poll::Lost,
        }
    }

    /// Wartet auf das Ergebnis (für Tests und Abläufe ohne Fenster). Nach `Ready` liefert eine
    /// weitere Abfrage `Lost`: den Auftrag danach verwerfen.
    pub fn wait(&self) -> Poll<T> {
        match self.rx.recv() {
            Ok(v) => Poll::Ready(v),
            Err(_) => Poll::Lost,
        }
    }

    /// Ergebnis abholen; mit `block` wird gewartet (Tests und Abläufe ohne Fenster).
    /// Nach `Ready` liefert jede weitere Abfrage `Lost`.
    pub fn fetch(&self, block: bool) -> Poll<T> {
        if block { self.wait() } else { self.poll() }
    }

    pub fn cancel(&self) {
        self.progress.cancel();
    }

    pub fn fraction(&self) -> f32 {
        self.progress.fraction()
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.progress.cancel();
    }
}
