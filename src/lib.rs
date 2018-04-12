extern crate futures;

pub mod stream_utils {
    use futures::{Future, Stream, Async, stream };

    pub fn numbers(from: i64) -> Box<Stream<Item=i64, Error=String>> {
        let mut counter = from;
        
        Box::new(stream::poll_fn(move || {
            let next = counter;
            counter += 1;

            Ok(Async::Ready(Some(next)))
        }))
    }

    #[cfg(test)]
    mod tests {
        mod stream_utils {
            use stream_utils::*;
            
            #[test]
            fn test_numbers() {
                let n = numbers(0);

                let v: Vec<i64> = n.take(5).collect().wait().unwrap();
                assert_eq!(v, vec![0,1,2,3,4]);
            }

            #[test]
            fn test_zip() {
                let a = numbers(0);
                let b = numbers(10);
                let v: Vec<(i64, i64)> = a.take(2).zip(b.take(5)).collect().wait().unwrap();

                assert_eq!(v, vec![(0, 10), (1, 11)]);
            }
        }
    }
}
