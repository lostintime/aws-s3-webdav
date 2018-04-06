mod utils {
    /**
     * Finds largest element of type T from the given list
     */
    fn largest<T>(list: &[T]) -> Option<T>
    where
        T: PartialOrd + Copy,
    {
        let mut max: Option<T> = None;

        for v in list.iter() {
            match max {
                Some(m) => {
                    if v > &m {
                        max = Some(*v)
                    }
                }
                None => max = Some(*v),
            }
        }

        max
    }

    #[derive(Debug)]
    struct Named {
        name: String,
    }

    impl Named {
        fn new(name: &str) -> Named {
            Named {
                name: String::from(name),
            }
        }
    }

    impl PartialEq for Named {
        fn eq(&self, other: &Named) -> bool {
            self.name == other.name
        }
    }

    #[cfg(test)]
    mod tests {
        mod utils {
            use utils::*;
            /**
             * Testing utils::largest() function
             */
            #[test]
            fn largest_test() {
                assert_eq!(Some(8), largest(&vec![1, 4, 3, 8]))
            }

            #[test]
            fn build_named() {
                assert_eq!(
                    String::from("John"),
                    Named::new("John").name,
                    ".name field is different when Named instance created via new() method"
                );
                assert_eq!(
                    Named {
                        name: String::from("Hello"),
                    },
                    Named::new("Hello")
                )
            }
        }
    }
}
