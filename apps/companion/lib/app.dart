import 'package:flutter/material.dart';

import 'features/proxy_safety/data/android_proxy_safety_repository.dart';
import 'features/proxy_safety/presentation/proxy_safety_screen.dart';
import 'features/proxy_safety/presentation/proxy_safety_view_model.dart';

class CompanionApp extends StatefulWidget {
  const CompanionApp({super.key});

  @override
  State<CompanionApp> createState() => _CompanionAppState();
}

class _CompanionAppState extends State<CompanionApp> {
  late final ProxySafetyViewModel _viewModel;

  @override
  void initState() {
    super.initState();
    _viewModel = ProxySafetyViewModel(AndroidProxySafetyRepository())..load();
  }

  @override
  void dispose() {
    _viewModel.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
        title: 'App Tester Companion',
        debugShowCheckedModeBanner: false,
        theme: ThemeData(
          colorScheme: ColorScheme.fromSeed(
            seedColor: const Color(0xff6954a3),
            brightness: Brightness.dark,
          ),
          useMaterial3: true,
        ),
        home: ProxySafetyScreen(viewModel: _viewModel),
      );
}
