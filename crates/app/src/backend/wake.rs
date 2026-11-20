//! Waking the main thread: a backend thread that has news for the window
//! sends it on a [`Teller`], which puts it in its channel and then asks the
//! main thread to look ([`Wake::wake`]), so no news waits for the window's
//! timer. A thread cannot send news and forget to wake: the teller does
//! both. Asking is cheap when a wake is already pending: one post per look,
//! however many threads send. The main thread clears the pending mark
//! ([`Wake::taken`]) before it drains the channels, so a send that races the
//! drain posts again. Pure: what a post does is the caller's
//! (`platform::waker`).

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SendError, Sender};

/// Asks the main thread to look at the news. Cloned into every backend
/// thread; [`Wake::none`] does nothing (tests, the preview).
#[derive(Clone, Default)]
pub struct Wake {
    post: Option<Arc<Post>>,
}

/// The function that asks the main thread to look, and whether it has
/// been asked since it last looked.
struct Post {
    post: Box<dyn Fn() + Send + Sync>,
    pending: AtomicBool,
}

impl Wake {
    /// Wakes by calling `post`, which asks the main thread to look; it may
    /// be called from any thread.
    pub fn new(post: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            post: Some(Arc::new(Post { post: Box::new(post), pending: AtomicBool::new(false) })),
        }
    }

    /// A wake that never posts: news waits for the timer (tests).
    #[cfg(test)]
    pub fn none() -> Self {
        Self::default()
    }

    /// Asks the main thread to look, unless it was asked and has not
    /// looked yet ([`Wake::taken`]).
    pub fn wake(&self) {
        if let Some(post) = &self.post
            && !post.pending.swap(true, Ordering::AcqRel)
        {
            (post.post)();
        }
    }

    /// The main thread is looking: the next wake posts again. Call it
    /// before draining the channels.
    pub fn taken(&self) {
        if let Some(post) = &self.post {
            post.pending.store(false, Ordering::Release);
        }
    }

    /// A channel whose sends wake the main thread.
    pub fn channel<T>(&self) -> (Teller<T>, Receiver<T>) {
        let (send, heard) = mpsc::channel();
        (Teller { send, wake: self.clone() }, heard)
    }
}

impl fmt::Debug for Wake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.post.is_some() { "Wake" } else { "Wake::none" })
    }
}

/// The sending half of a [`Wake::channel`].
pub struct Teller<T> {
    send: Sender<T>,
    wake: Wake,
}

impl<T> Teller<T> {
    /// Sends `news`, then wakes the main thread.
    ///
    /// # Errors
    ///
    /// The receiver is gone: nobody listens, and nobody is woken.
    pub fn send(&self, news: T) -> Result<(), SendError<T>> {
        self.send.send(news)?;
        self.wake.wake();
        Ok(())
    }
}

impl<T> Clone for Teller<T> {
    fn clone(&self) -> Self {
        Self { send: self.send.clone(), wake: self.wake.clone() }
    }
}

impl<T> fmt::Debug for Teller<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Teller").field("wake", &self.wake).finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// A wake that counts its posts.
    pub(crate) fn counting() -> (Wake, Arc<AtomicUsize>) {
        let posts = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&posts);
        (Wake::new(move || _ = counted.fetch_add(1, Ordering::SeqCst)), posts)
    }

    #[test]
    fn two_wakes_post_once_and_after_taken_the_next_posts_again() {
        let (wake, posts) = counting();
        wake.wake();
        wake.clone().wake();
        assert_eq!(posts.load(Ordering::SeqCst), 1);
        wake.taken();
        wake.wake();
        assert_eq!(posts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_teller_wakes_after_its_news_is_in_the_channel() {
        type Slot = Mutex<Option<Receiver<u32>>>;
        let (heard, saw): (Arc<Slot>, _) = (Arc::default(), Arc::new(Mutex::new(None)));
        let (looks, saw_it) = (Arc::clone(&heard), Arc::clone(&saw));
        // The post looks in the channel, as the main thread will.
        let wake = Wake::new(move || {
            let took = looks.lock().unwrap().as_ref().and_then(|r| r.try_recv().ok());
            *saw_it.lock().unwrap() = took;
        });
        let (tell, receiver) = wake.channel::<u32>();
        *heard.lock().unwrap() = Some(receiver);
        tell.send(7).unwrap();
        assert_eq!(*saw.lock().unwrap(), Some(7), "the news was there when the post came");
    }

    #[test]
    fn a_teller_whose_receiver_is_gone_fails_and_does_not_wake() {
        let (wake, posts) = counting();
        let (tell, heard) = wake.channel::<u32>();
        drop(heard);
        assert!(tell.send(1).is_err());
        assert_eq!(posts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn none_is_silent() {
        let wake = Wake::none();
        wake.wake();
        wake.taken();
        let (tell, heard) = wake.channel();
        tell.send("x").unwrap();
        assert_eq!(heard.try_recv(), Ok("x"));
        assert_eq!(format!("{wake:?}"), "Wake::none");
    }
}
