macro_rules! head {
    ($head:ty) => {
        $head
    };
    ($head:ty, $($_tail:ty),+) => {
        $head
    };
}

macro_rules! head_or_tail {
    (
        ($h0:expr, $_t0:expr)
    ) => {
        ($h0,)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr)
    ) => {
        ($h0, $t1)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr)
    ) => {
        ($h0, $t1, $t2)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr)
    ) => {
        ($h0, $t1, $t2, $t3)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr),
        ($_h7:expr, $t7:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6, $t7)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr),
        ($_h7:expr, $t7:expr),
        ($_h8:expr, $t8:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6, $t7, $t8)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr),
        ($_h7:expr, $t7:expr),
        ($_h8:expr, $t8:expr),
        ($_h9:expr, $t9:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6, $t7, $t8, $t9)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr),
        ($_h7:expr, $t7:expr),
        ($_h8:expr, $t8:expr),
        ($_h9:expr, $t9:expr),
        ($_h10:expr, $t10:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6, $t7, $t8, $t9, $t10)
    };
    (
        ($h0:expr, $_t0:expr),
        ($_h1:expr, $t1:expr),
        ($_h2:expr, $t2:expr),
        ($_h3:expr, $t3:expr),
        ($_h4:expr, $t4:expr),
        ($_h5:expr, $t5:expr),
        ($_h6:expr, $t6:expr),
        ($_h7:expr, $t7:expr),
        ($_h8:expr, $t8:expr),
        ($_h9:expr, $t9:expr),
        ($_h10:expr, $t10:expr),
        ($_h11:expr, $t11:expr)
    ) => {
        ($h0, $t1, $t2, $t3, $t4, $t5, $t6, $t7, $t8, $t9, $t10, $t11)
    };
}

pub(super) use {head, head_or_tail};
