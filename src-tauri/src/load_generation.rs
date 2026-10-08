//! Publish a decoded image only while its selection is still current.
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn replace_if_current<T>(
    slot: &Mutex<Option<T>>,
    generation: &AtomicUsize,
    expected: usize,
    image: T,
) -> bool {
    // New loads increment the generation before clearing this same slot. Check
    // while holding its lock so a stale result cannot overwrite a newer image.
    let mut current = slot.lock().unwrap_or_else(|error| error.into_inner());
    if generation.load(Ordering::SeqCst) != expected {
        return false;
    }
    *current = Some(image);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn current_load_can_publish() {
        let slot = Mutex::new(None);
        assert!(replace_if_current(&slot, &AtomicUsize::new(3), 3, "photo"));
        assert_eq!(*slot.lock().unwrap(), Some("photo"));
    }

    #[test]
    fn late_white_balance_or_decode_cannot_replace_a_newer_photo() {
        let slot = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicUsize::new(1));
        let finished_metadata = Arc::new(Barrier::new(2));
        let worker = {
            let slot = slot.clone();
            let generation = generation.clone();
            let finished_metadata = finished_metadata.clone();
            std::thread::spawn(move || {
                // The first load was current before an expensive metadata read.
                finished_metadata.wait();
                replace_if_current(&slot, &generation, 1, "old photo")
            })
        };
        generation.fetch_add(1, Ordering::SeqCst);
        *slot.lock().unwrap() = None;
        assert!(replace_if_current(&slot, &generation, 2, "new photo"));
        finished_metadata.wait();
        assert!(!worker.join().unwrap());
        assert_eq!(*slot.lock().unwrap(), Some("new photo"));
    }

    #[test]
    fn stale_load_does_not_fill_a_slot_cleared_for_the_next_selection() {
        let slot: Mutex<Option<&str>> = Mutex::new(None);
        assert!(!replace_if_current(
            &slot,
            &AtomicUsize::new(8),
            7,
            "old photo"
        ));
        assert_eq!(*slot.lock().unwrap(), None);
    }
}
