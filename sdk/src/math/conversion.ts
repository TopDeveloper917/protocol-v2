import { BN } from '../';
import { MARK_PRICE_PRECISION } from '../constants/numericConstants';

export const convertToNumber = (
	bigNumber: BN,
	precision: BN = MARK_PRICE_PRECISION
) => {
	if (!bigNumber) return 0;
	return (
		bigNumber.div(precision).toNumber() +
		bigNumber.mod(precision).toNumber() / precision.toNumber()
	);
};
