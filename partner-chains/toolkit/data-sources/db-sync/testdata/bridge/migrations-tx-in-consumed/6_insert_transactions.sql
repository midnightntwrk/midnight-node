DO $$
DECLARE

 unit_datum jsonb := '{ "constructor": 0, "fields": [] }';
 reserve_metadatum jsonb := '[]';
 transfer_metadatum_1 jsonb := '["0xabcd"]';
 transfer_metadatum_2 jsonb := '["0x1234"]';
 invalid_metadatum jsonb := '{ "it is no a string": "but a map" }';
 reserve_and_user_tx_metadatum jsonb := '["0x9999"]';

 native_token_policy hash28type := decode('500000000000000000000000000000000000434845434b504f494e69', 'hex');
 native_token_id integer := 1;
 irrelevant_token_id integer := 2;

 init_ics_tx integer = 1;
 reserve_transfer_tx integer   := 10;
 user_transfer_tx_1 integer := 21;
 user_transfer_tx_2 integer  := 22;
 invalid_transfer_tx_1 integer  := 31;
 invalid_transfer_tx_2 integer  := 32;
 irrelevant_tx integer  := 41;
 complex_withdraw_tx integer  := 51;
 reserve_and_user_tx integer  := 61;

 -- those hashes are not really important but putting them in variables help to make the data more readable
 init_ics_tx_hash hash32type := decode('c000000000000000000000000000000000000000000000000000000000000001','hex');
 reserve_transfer_tx_hash hash32type := decode('c000000000000000000000000000000000000000000000000000000000000002','hex');
 user_transfer_tx_hash_1 hash32type := decode('c000000000000000000000000000000000000000000000000000000000000003','hex');
 user_transfer_tx_hash_2 hash32type := decode('c000000000000000000000000000000000000000000000000000000000000004','hex');
 ivalid_transfer_tx_hash_1 hash32type := decode('c000000000000000000000000000000000000000000000000000000000000005','hex');
 ivalid_transfer_tx_hash_2 hash32type := decode('c000000000000000000000000000000000000000000000000000000000000006','hex');
 complex_withdraw_tx_hash hash32type := decode('c000000000000000000000000000000000000000000000000000000000000007','hex');
 reserve_and_user_tx_hash hash32type := decode('c000000000000000000000000000000000000000000000000000000000000008','hex');
 irrelevant_tx_hash hash32type := decode('4242424242424242424242424242424242424242424242424242424242424242','hex');

 unit_datum_hash hash32type := decode('0000000000000000000000000000000000000000000000000000000000000001','hex');

BEGIN

INSERT INTO tx ( id                    , hash                       , block_id, block_index, out_sum, fee, deposit, size, invalid_before, invalid_hereafter, valid_contract, script_size )
    VALUES     ( init_ics_tx           , init_ics_tx_hash           , 1       , 0          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( irrelevant_tx         , irrelevant_tx_hash         , 1       , 1          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( reserve_transfer_tx   , reserve_transfer_tx_hash   , 2       , 0          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( user_transfer_tx_1    , user_transfer_tx_hash_1    , 2       , 1          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( user_transfer_tx_2    , user_transfer_tx_hash_2    , 4       , 0          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( invalid_transfer_tx_1 , ivalid_transfer_tx_hash_1  , 4       , 1          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( invalid_transfer_tx_2 , ivalid_transfer_tx_hash_2  , 4       , 2          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( complex_withdraw_tx   , complex_withdraw_tx_hash   , 5       , 0          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
              ,( reserve_and_user_tx   , reserve_and_user_tx_hash   , 5       , 1          , 0      , 0  , 0      , 1024, NULL          , NULL             , TRUE          , 1024        )
;

-- reserve_transfer_tx consumes 250 from 'reserve address' and deposits '100' at 'ics address' and '150' at 'reserve address' (change)
-- irrelevant_tx funds 'non-ics addr' with some tokens that are consumed later
-- user_transfer_tx_1 consumes 100 from 'ics address' and 10 from 'user address' and deposits 110 at 'ics address' (net +10)
-- user_transfer_tx_2 consumes 110 from 'ics address' and 10 from 'non-ics addr' and deposits 120 at 'ics address' (net +10)
-- invalid_transfer_tx_1 consumes 120 from 'ics address' and 880 from 'user address' and deposits 1000 at 'ics address' (net +800), it has invalid metadata
-- invalid_transfer_tx_2 consumes 1000 from 'user address' and deposits 1000 at 'ics address' (net +1000), it doesn't have metadata
-- complex_withdraw_tx is edge case of transaction that withdraws 50 from reserve (system assumption is that it goes to ICS), it also withdraws 55 from ICS (currently impossible with bidirectional bridge),
--   in this case ICS net is -5, reserve net is -50, user net is +55, it should be reflected one event: Reserve Transfer of 50 because ICS withdrawals are out-of-scope

INSERT INTO tx_out ( id, tx_id                 , index, address           , address_raw, address_has_script, payment_cred, stake_address_id, value, data_hash                   , consumed_by_tx_id     )
            VALUES ( 11, init_ics_tx           , 0    , 'reserve address' , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , reserve_transfer_tx   ) -- Reserve initial utxo - 250 tokens
                  ,( 12, init_ics_tx           , 1    , 'user address'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , user_transfer_tx_1    ) -- 10 tokens, spend by user_transfer_tx_1
                  ,( 13, init_ics_tx           , 2    , 'user address'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , invalid_transfer_tx_1 ) -- 880 tokens, spend by invalid_transfer_tx_1
                  ,( 14, init_ics_tx           , 3    , 'user address'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , invalid_transfer_tx_2 ) -- 1000 tokens, spend by invalid_transfer_tx_2
                  ,( 15, irrelevant_tx         , 0    , 'non-ics addr'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , user_transfer_tx_2    ) -- 10 tokens, spend by user_transfer_tx_2
                  ,( 16, irrelevant_tx         , 1    , 'non-ics addr'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , complex_withdraw_tx   ) -- 10 tokens, spend by complex_withdraw_tx
                  ,( 21, reserve_transfer_tx   , 0    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , user_transfer_tx_1    ) -- output of reserve to ICS - 100 tokens
                  ,( 22, reserve_transfer_tx   , 1    , 'reserve address' , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , complex_withdraw_tx   ) -- change of reserve to ICS transfer - 150 tokens
                  ,( 31, user_transfer_tx_1    , 0    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , user_transfer_tx_2    ) -- output of user_transfer_tx_1: 100 + 10 = 110 tokens
                  ,( 32, user_transfer_tx_2    , 1    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , invalid_transfer_tx_1 ) -- output of user_transfer_tx_2: 110 + 10 = 120 tokens
                  ,( 41, invalid_transfer_tx_1 , 0    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , complex_withdraw_tx   ) -- output of invalid_transfer_tx_1: 880 + 120 = 1000 tokens
                  ,( 42, invalid_transfer_tx_2 , 0    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , complex_withdraw_tx   ) -- output of invalid_transfer_tx_2: 1000 tokens
                  ,( 51, complex_withdraw_tx   , 0    , 'user address'    , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , reserve_and_user_tx   ) -- user output of complex_withdraw_tx: 65 tokens
                  ,( 52, complex_withdraw_tx   , 1    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , reserve_and_user_tx   ) -- ics output of complex_withdraw_tx: 1995 tokens
                  ,( 53, complex_withdraw_tx   , 2    , 'reserve address' , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , reserve_and_user_tx   ) -- reserve output of complex_withdraw_tx: 100 tokens
                  ,( 54, reserve_and_user_tx   , 0    , 'ics address'     , ''         , TRUE              , NULL        , NULL            , 0    , NULL                        , NULL                  )


;

INSERT INTO datum ( id, hash                       , tx_id        , value         )
           VALUES ( 0 , unit_datum_hash , reserve_transfer_tx     , unit_datum    )
                , ( 1 , unit_datum_hash , user_transfer_tx_1      , unit_datum    )
                , ( 2 , unit_datum_hash , user_transfer_tx_2      , unit_datum    )
                , ( 3 , unit_datum_hash , invalid_transfer_tx_1   , unit_datum    )
;

INSERT INTO tx_metadata ( id , "key"   , json                          , bytes , tx_id                 )
	             VALUES ( 0  , 6500973 , reserve_metadatum             , ''    , reserve_transfer_tx   )
	                  , ( 1  , 6500973 , transfer_metadatum_1          , ''    , user_transfer_tx_1    )
	                  , ( 2  , 6500973 , transfer_metadatum_2          , ''    , user_transfer_tx_2    )
	                  , ( 3  , 6500973 , invalid_metadatum             , ''    , invalid_transfer_tx_1 )
					  , ( 4  , 6500973 , reserve_and_user_tx_metadatum , ''    , reserve_and_user_tx   )
;

INSERT INTO multi_asset ( id                  , policy                  , name               , fingerprint       )
VALUES                  ( native_token_id     , native_token_policy     , 'native token'     , 'nativeToken'     )
                       ,( irrelevant_token_id , native_token_policy     , 'irrelevant token' , 'irrelevantToken' )
;

INSERT INTO ma_tx_out (id , quantity , tx_out_id , ident )
VALUES                (11 , 250      , 11        , native_token_id )  -- initial reserve state is 250
                     ,(12 , 10       , 12        , native_token_id ) -- initial value at user address
                     ,(13 , 880      , 13        , native_token_id ) -- initial value at user address
                     ,(14 , 1000     , 14        , native_token_id ) -- initial value at user address
                     ,(15 , 10       , 15        , native_token_id ) -- at 'non-ics addr', 10 tokens consumed by user_transfer_tx_2
                     ,(16 , 10       , 16        , native_token_id ) -- at 'non-ics addr', 10 tokens consumed by complex_withdraw_tx
                     ,(17 , 1000     , 15        , irrelevant_token_id )
					 ,(18 , 100      , 21        , native_token_id )  -- output from reserve to ICS
					 ,(19 , 9999     , 21        , irrelevant_token_id )
					 ,(20 , 150      , 22        , native_token_id )  -- change of reserve to ICS transfer
                     ,(21 , 110      , 31        , native_token_id )  -- output of user_transfer_tx_1 110 = 10 (from user) + 100 (from ICS), net addition to ICS is 10
                     ,(22 , 9999     , 31        , irrelevant_token_id )
                     ,(23 , 120      , 32        , native_token_id ) -- output of user_transfer_tx_1 120 = 10 (from user) + 110 (from ICS), net addition to ICS is 10
                     ,(24 , 9999     , 32        , irrelevant_token_id )
                     ,(25 , 1000     , 41        , native_token_id ) -- ics output of invalid_transfer_tx_1
                     ,(26 , 9999     , 41        , irrelevant_token_id )
                     ,(27 , 1000     , 42        , native_token_id ) -- ics output of invalid_transfer_tx_2
                     ,(28 , 65       , 51        , native_token_id ) -- user output of complex_withdraw_tx
                     ,(29 , 1995     , 52        , native_token_id ) -- ics output of complex_withdraw_tx
                     ,(30 , 100      , 53        , native_token_id ) -- reserve output of complex_withdraw_tx
                     ,(31 , 2160     , 54        , native_token_id ) -- ICS output of reserve_and_user_tx
;
END $$;
