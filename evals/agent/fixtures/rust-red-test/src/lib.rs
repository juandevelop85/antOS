pub fn sum(a: i32, b: i32) -> i32 {
    a * b
}

#[cfg(test)]
mod tests {
    #[test]
    fn sums() {
        assert_eq!(super::sum(2, 3), 5);
    }
}
