fn main() {
    1..2;   // std::ops::Range
    3..;    // std::ops::RangeFrom
    ..4;    // std::ops::RangeTo
    ..;     // std::ops::RangeFull

    // still feature gated
    // 1...2;   // std::ops::RangeInclusive
    // ...4;    // std::ops::RangeToInclusive
}
