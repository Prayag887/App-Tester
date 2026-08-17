import 'package:flutter/material.dart';

import '../../../shared/brand/app_tester_mark.dart';
import 'proxy_safety_view_model.dart';

class ProxySafetyScreen extends StatefulWidget {
  const ProxySafetyScreen({required this.viewModel, super.key});

  final ProxySafetyViewModel viewModel;

  @override
  State<ProxySafetyScreen> createState() => _ProxySafetyScreenState();
}

class _ProxySafetyScreenState extends State<ProxySafetyScreen> {
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.viewModel,
        builder: (context, _) {
          final model = widget.viewModel;
          final active = model.status?.isVpnActive == true;
          return Scaffold(
            body: SafeArea(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 520),
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const Row(children: [
                            AppTesterMark(size: 44),
                            SizedBox(width: 12),
                            Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Companion',
                                      style: TextStyle(
                                          fontSize: 20,
                                          fontWeight: FontWeight.w700)),
                                  Text('USB capture status',
                                      style:
                                          TextStyle(color: Color(0xff8ea6c9))),
                                ]),
                          ]),
                          const SizedBox(height: 30),
                          _StatusCard(
                              active: active,
                              packageName: model.status?.targetPackage),
                          const SizedBox(height: 20),
                          Text('USB connection',
                              style: Theme.of(context)
                                  .textTheme
                                  .titleMedium
                                  ?.copyWith(fontWeight: FontWeight.w700)),
                          const SizedBox(height: 12),
                          const Text(
                              'Keep this device connected by USB. Select the app and start capture from App Tester on the desktop.',
                              style: TextStyle(
                                  color: Color(0xffaebfd8), height: 1.5)),
                          if (model.error != null) ...[
                            const SizedBox(height: 14),
                            Text(model.error!,
                                style:
                                    const TextStyle(color: Color(0xffff9bac))),
                          ],
                          const SizedBox(height: 22),
                          if (active)
                            FilledButton.icon(
                              onPressed: model.isWorking
                                  ? null
                                  : () => model.stopVpn(),
                              icon: const Icon(Icons.stop_circle_outlined),
                              label: const Text('Stop VPN'),
                              style: FilledButton.styleFrom(
                                  minimumSize: const Size.fromHeight(52),
                                  backgroundColor: const Color(0xff9e3d4d)),
                            ),
                        ]),
                  ),
                ),
              ),
            ),
          );
        },
      );
}

class _StatusCard extends StatelessWidget {
  const _StatusCard({required this.active, required this.packageName});

  final bool active;
  final String? packageName;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.all(24),
        decoration: BoxDecoration(
          color: active ? const Color(0xff103831) : const Color(0xff0d1d35),
          borderRadius: BorderRadius.circular(22),
          border: Border.all(
              color:
                  active ? const Color(0xff246f5d) : const Color(0xff203651)),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Icon(active ? Icons.shield_rounded : Icons.phone_android_rounded,
              size: 38,
              color:
                  active ? const Color(0xff2be0a7) : const Color(0xff82baff)),
          const SizedBox(height: 14),
          Text(active ? 'VPN capture active' : 'VPN capture is off',
              style: Theme.of(context)
                  .textTheme
                  .titleLarge
                  ?.copyWith(fontWeight: FontWeight.w700)),
          const SizedBox(height: 7),
          Text(
              active
                  ? 'Capturing traffic from ${packageName ?? 'the selected package'}.'
                  : 'Connect USB and start capture from the desktop app.',
              style: const TextStyle(color: Color(0xffaebfd8), height: 1.4)),
        ]),
      );
}
