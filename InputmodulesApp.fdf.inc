## @file
#  Framework Laptop 16 FDF include file the input module detection application
#
#  This file can be included to the [FV*] section(s) of a platform FDF file
#  by using "!include InputmodulesApp.fdf.inc" to specify the module instances
#  to be built in the firmware volume.
#
#  Copyright (c) 2023 Framework Computer Inc
#
#  SPDX-License-Identifier: BSD-2-Clause-Patent
#
#
##

FILE APPLICATION = 40DA8F9B-A7DD-423E-966C-269CCFEE445E {
  SECTION PE32 = inputmodules_0.1.0.efi
  SECTION UI = "InputmodulesApp"
}
