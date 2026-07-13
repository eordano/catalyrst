BEGIN;
CREATE TABLE IF NOT EXISTS squid_marketplace.third_party (
    "id" character varying NOT NULL,
    "root" text,
    "is_approved" boolean NOT NULL,
    CONSTRAINT "PK_third_party" PRIMARY KEY ("id")
);
INSERT INTO squid_marketplace.third_party (id, root, is_approved) VALUES
  ('urn:decentraland:matic:collections-thirdparty:a-kid-called-beast-inc', '0x44af6f9a430f933646e5c3b6f34d22c1a1c24f46b6996b5ddd7e8efd08e8634c', true),
  ('urn:decentraland:matic:collections-thirdparty:actual-3d-punks', NULL, false),
  ('urn:decentraland:matic:collections-thirdparty:adidas-virtual-gear', '0xaf42fcd56de07994a79f7dfc56d182aa72084635235244a91f2be72bf81c91b7', true),
  ('urn:decentraland:matic:collections-thirdparty:avant-garde---ha-jung-woo-x-supernormal', '0xfa9b631b87f495e25d85d8da211beb7b9483443aa8e797a4ccd146936669d23d', true),
  ('urn:decentraland:matic:collections-thirdparty:baby-doge-coin', '0x520935982725741b52273556152c77a2adaceb4f8c9be4262b99c7e6f8c0f9ec', true),
  ('urn:decentraland:matic:collections-thirdparty:bnv-fashion', '0x8ac9a1fa488328114a432ddc717c9f41d271a9e7e47ea57c54515514ea71d7b0', true),
  ('urn:decentraland:matic:collections-thirdparty:cryptoavatars', '0xf72a53e92ad5e9bfb3c190c112142a425847808a9b5ab7addc10e9931e816aa7', true),
  ('urn:decentraland:matic:collections-thirdparty:dolcegabbana-disco-drip', '0xbb278f53523bf43e649b0b29b3bb61bf5bf7cf4fc6f7b63e87aa226df612c4c4', true),
  ('urn:decentraland:matic:collections-thirdparty:dressx-digital-fashion--wearable-nfts', '0xc13ca2a26668a9eddeda5ebeba85442224f1684f0b1a7b225e59654f50194583', true),
  ('urn:decentraland:matic:collections-thirdparty:endstate', '0xb64e65aef8b2c1ca11807ade6b84b2f1739d901693fb3010a2684a7f800c5374', true),
  ('urn:decentraland:matic:collections-thirdparty:kollectiff', '0x475aef9921b509ba329525868974822e92c0948f3ff9c4291d57d3575031fe7b', true),
  ('urn:decentraland:matic:collections-thirdparty:metadogenft', '0xbf0968658f84492af3863affa980af6cb3883cb6507ba2e815651801668c1628', true),
  ('urn:decentraland:matic:collections-thirdparty:metawardrobe-virtual-mens-fashion', '0xcd517c9309574c0778ab83233652ed7083941be17c920b1e15ea8fdd62ea5cd0', true),
  ('urn:decentraland:matic:collections-thirdparty:metawardrobe-virtual-womens-fashion', '0x6b591453e82be90735753b5dd37da938134895fe3b62d1b18be5ae332dff0747', true),
  ('urn:decentraland:matic:collections-thirdparty:nft-studios', '0x992b07c458d62fba3db1edc979a1a9eba1a90a73248e961aeac19abe7878eb04', true),
  ('urn:decentraland:matic:collections-thirdparty:no-name-studios', NULL, false),
  ('urn:decentraland:matic:collections-thirdparty:ntr1-meta', '0x0eef0954f9ebd4dd771920a075e64c30ccad391e5f068363ae9c5f62be42efd5', true),
  ('urn:decentraland:matic:collections-thirdparty:nyan-cat', '0xf1276838b0e228ca9f14e5f32f8fe7087d5e82f934bd9f8599bf62ec7e6f309a', true),
  ('urn:decentraland:matic:collections-thirdparty:onchainchain', NULL, false),
  ('urn:decentraland:matic:collections-thirdparty:project-nayom1', '0x96f02b5cd9050c71eb404a62bb614b93b0788ed51cc6a6c664121895a07ad7ea', true),
  ('urn:decentraland:matic:collections-thirdparty:rekt-vandalized-clone-9b9f', '0x4d0158c104edba08f29f068f608e4daac73b9106fbae68a24ec04f265a73c97c', true),
  ('urn:decentraland:matic:collections-thirdparty:reserva-x---spriz', '0xebea9abc205142346ab010588c5b93cf9968163de1afea55e86011b7188afa31', true),
  ('urn:decentraland:matic:collections-thirdparty:satoshiverse', '0xa51c0b4aee431907d8a24e7feb3ee44771cdbce814063d49b08025e64565b563', true),
  ('urn:decentraland:matic:collections-thirdparty:waifumon-gen-2', '0x3d73f09f4bdf74b615ad86895f38740dd5b23f9f43900cc7227f0dc0a9fcbc4b', true),
  ('urn:decentraland:matic:collections-thirdparty:woodies', '0x490c5f9c5d37303e27532b2ec348ad21d560b39fba4833a4b3f10a6647bb1416', true),
  ('urn:decentraland:polygon:collections-thirdparty:kollectiff', NULL, false),
  ('urn:decentraland:polygon:collections-thirdparty:satoshiverse', NULL, false)
ON CONFLICT (id) DO UPDATE SET root = EXCLUDED.root, is_approved = EXCLUDED.is_approved;
COMMIT;
