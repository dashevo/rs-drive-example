use crate::Explorer;
use rand::Rng;

pub struct Masternode {
    pub pro_tx_hash: [u8; 32],
}

impl Masternode {
    pub(crate) fn new_random() -> Masternode {
        let pro_tx_hash = rand::random::<[u8; 32]>();
        Masternode { pro_tx_hash }
    }

    pub(crate) fn new_random_many(count: usize) -> Vec<Masternode> {
        (0..count).into_iter().map(|_| Self::new_random()).collect()
    }
}

impl Explorer {
    pub(crate) fn random_masternode(&self) -> &Masternode {
        let mut rng = rand::thread_rng();
        let index: usize = rng.gen_range(0..self.masternodes.len());
        self.masternodes.get_index(index).unwrap().1
    }
}
