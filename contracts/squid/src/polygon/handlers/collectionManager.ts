import { Network } from "@dcl/schemas";
import { getAddresses } from "../../common/utils/addresses";
import { RaritiesSetEventArgs } from "../abi/CollectionManager";
import * as RarityABI from "../abi/Rarity";
import * as RaritiesWithOracleABI from "../abi/RaritiesWithOracle";
import { Currency, Rarity } from "../../model";
import { Block, Context } from "../processor";

export async function handleRaritiesSet(
  ctx: Context,
  block: Block,
  event: RaritiesSetEventArgs,
  rarities: Map<string, Rarity>
): Promise<void> {
  const addresses = getAddresses(Network.MATIC);
  const rarityAddress = addresses.Rarity;
  const raritiesWithOracleAddress = addresses.RaritiesWithOracle;
  const newRaritiesAddress = event._newRarities;

  if (newRaritiesAddress === raritiesWithOracleAddress) {
    let raritiesWithOracle = new RaritiesWithOracleABI.Contract(
      ctx,
      block,
      raritiesWithOracleAddress
    );
    let raritiesCount = await raritiesWithOracle.raritiesCount();

    for (let i = 0; i < raritiesCount; i++) {
      const blockchainRarity = await raritiesWithOracle.rarities(BigInt(i));
      const name = blockchainRarity.name;
      let rarity = rarities.get(name);

      if (!rarity) {
        rarity = new Rarity({ id: name });
      }

      rarity.name = name;
      rarity.price = blockchainRarity.price;
      rarity.maxSupply = blockchainRarity.maxSupply;
      rarity.currency = Currency.USD;
      rarities.set(rarity.name, rarity);
      //   rarity.save();
    }
  } else if (newRaritiesAddress === rarityAddress) {
    const rarityContract = new RarityABI.Contract(ctx, block, rarityAddress);
    const raritiesCount = await rarityContract.raritiesCount();

    for (let i = 0; i < raritiesCount; i++) {
      const blockchainRarity = await rarityContract.rarities(BigInt(i));
      const name = blockchainRarity.name;
      let rarity = rarities.get(name);

      if (!rarity) {
        rarity = new Rarity({ id: name });
      }

      rarity.name = blockchainRarity.name;
      rarity.price = blockchainRarity.price;
      rarity.maxSupply = blockchainRarity.maxSupply;
      rarity.currency = Currency.MANA;
      rarities.set(rarity.name, rarity);
      //   rarity.save();
    }
  } else {
    console.log(
      `INFO: Unsupported rarity contract address ${newRaritiesAddress}`
    );
  }
}
