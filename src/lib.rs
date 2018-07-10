// TODO: check if could use weaker ordering than SeqCst.

use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

struct Node<T> {
    next: AtomicPtr<Node<T>>,
    value: Option<T>,
}

impl<T> Node<T> {
    fn new(value: T) -> Self {
        Node {
            next: AtomicPtr::new(ptr::null_mut()),
            value: Some(value),
        }
    }

    fn sentinel() -> Self {
        Node {
            next: AtomicPtr::new(ptr::null_mut()),
            value: None,
        }
    }
}

pub struct Queue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        let pointer = Box::into_raw(Box::new(Node::sentinel()));
        Self {
            head: AtomicPtr::new(pointer),
            tail: AtomicPtr::new(pointer),
        }
    }

    pub fn enqueue(&self, value: T) {
        let new_tail = Box::into_raw(Box::new(Node::new(value)));
        let mut tail;
        loop {
            //println!("Enqueue");
            tail = self.tail.load(Ordering::SeqCst);
            unsafe {
                let true_tail = (*tail).next.load(Ordering::SeqCst);
                if !true_tail.is_null() {
                    // If the tail field has not yet been updated by another thread, help it to do
                    // so.
                    self.tail.compare_and_swap(tail, true_tail, Ordering::SeqCst);
                }
                if (*tail).next.compare_and_swap(ptr::null_mut(), new_tail, Ordering::SeqCst) != ptr::null_mut() {
                    // We were unable to add the element to the queue.
                    // We need to start the whole process again because the queue could have been
                    // cleared meanwhile.
                    continue;
                }
            }
            break;
        }
        // We don't know whether another thread added an element before of after the one we are
        // currently adding, so there's no point in trying to set the tail multiple times.
        self.tail.compare_and_swap(tail, new_tail, Ordering::SeqCst);
    }

    pub fn dequeue(&self) -> Option<T> {
        loop {
            //println!("Dequeue");
            let head = self.head.load(Ordering::SeqCst);
            let tail = self.tail.load(Ordering::SeqCst);
            unsafe {
                let first_node = (*head).next.load(Ordering::SeqCst);
                if head == tail {
                    if first_node.is_null() {
                        // The list is observed to be empty.
                        break;
                    }
                    self.tail.compare_and_swap(tail, first_node, Ordering::SeqCst);
                }
                else {
                    assert!(!first_node.is_null());
                    let new_first_node = (*first_node).next.load(Ordering::SeqCst);
                    if (*head).next.compare_and_swap(first_node, new_first_node, Ordering::SeqCst) == first_node {
                        // We were able to remove the first element.
                        if new_first_node.is_null() {
                            // If we removed the last element, set the tail to be equal to the head.
                            self.tail.compare_and_swap(tail, head, Ordering::SeqCst);
                        }
                        // TODO: add the node to the free list.
                        return (*first_node).value.take();
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;

    use super::Queue;

    #[test]
    fn test_single_thread() {
        let queue = Queue::new();
        queue.enqueue(10);
        assert_eq!(queue.dequeue(), Some(10));
        assert_eq!(queue.dequeue(), None);

        queue.enqueue(11);
        queue.enqueue(12);
        queue.enqueue(13);
        assert_eq!(queue.dequeue(), Some(11));
        assert_eq!(queue.dequeue(), Some(12));
        assert_eq!(queue.dequeue(), Some(13));
        assert_eq!(queue.dequeue(), None);

        queue.enqueue(14);
        queue.enqueue(15);
        assert_eq!(queue.dequeue(), Some(14));
        queue.enqueue(16);
        assert_eq!(queue.dequeue(), Some(15));
        assert_eq!(queue.dequeue(), Some(16));
        assert_eq!(queue.dequeue(), None);
    }

    #[test]
    fn test_multithread() {
        let queue = Arc::new(Queue::new());

        let results = Arc::new(Mutex::new(vec![]));

        /*let (sender, receiver) = sync_channel(1000);

        thread::spawn(move || {
            loop {
                if let Ok(msg) = receiver.recv() {
                    println!("{}", msg);
                }
            }
        });*/

        let handle = {
            let queue = queue.clone();
            let results = results.clone();
            thread::spawn(move || {
                let mut elements = vec![];
                //thread::yield_now();
                //let mut i = 0;
                //while i < 50_000 {
                for _ in 0..50_000 {
                    //sender.send("Thread dequeue");
                    if let Some(element) = queue.dequeue() {
                        elements.push(element);
                        //i += 1;
                    }
                }
                thread::sleep_ms(1000);
                //i = 0;
                //while i < 950_000 {
                for _ in 0..950_000 {
                    //sender.send("Thread dequeue");
                    if let Some(element) = queue.dequeue() {
                        elements.push(element);
                        //i += 1;
                    }
                }
                *results.lock().expect("lock") = elements;
            })
        };

        {
            let queue = queue.clone();
            thread::spawn(move || {
                for i in 0..100_000 {
                    queue.enqueue(i);
                }
            });
        }

        {
            let queue = queue.clone();
            thread::spawn(move || {
                for i in 100_000..1_000_000 {
                    queue.enqueue(i);
                }
            });
        }

        handle.join().expect("join");

        let mut results = results.lock().expect("lock");
        assert_eq!(results.len(), 1_000_000);

        results.sort();

        for i in 0..1_000_000 {
            assert_eq!(results[i], i);
        }
    }
}
